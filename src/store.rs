use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Store {
    pub root: PathBuf,
}

pub fn default_root() -> PathBuf {
    if let Ok(p) = std::env::var("AIYOU_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aiyou")
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self { root: root.to_path_buf() })
    }

    pub fn init(&self) -> Result<()> {
        std::fs::create_dir_all(self.chats_dir())?;
        std::fs::create_dir_all(self.profile_dir())?;
        std::fs::create_dir_all(self.imports_dir())?;
        std::fs::create_dir_all(self.runs_dir())?;
        if !self.config_path().exists() {
            crate::config::Config::default().save(&self.config_path())?;
        }
        crate::git::git_init(&self.root)?;
        crate::git::git_commit_all(&self.root, "init: aiyou store")?;
        Ok(())
    }

    pub fn chats_dir(&self) -> PathBuf {
        self.root.join("chats")
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.root.join("profile")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.profile_dir().join("runs")
    }

    pub fn imports_dir(&self) -> PathBuf {
        self.root.join("imports")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("aiyou.toml")
    }

    pub fn fingerprints_path(&self) -> PathBuf {
        self.root.join(".fingerprints.json")
    }

    pub fn traits_path(&self) -> PathBuf {
        self.profile_dir().join("traits.json")
    }

    pub fn skill_path(&self) -> PathBuf {
        self.profile_dir().join("SKILL.md")
    }

    pub fn session_path(&self, source: &str, id: &str, ext: &str) -> PathBuf {
        self.chats_dir().join(source).join(format!("{id}.{ext}"))
    }

    fn sanitize(&self, s: &str) -> String {
        crate::render::slug(s)
    }

    pub fn write_session(
        &self,
        meta: &crate::model::SessionMeta,
        msgs: &mut Vec<crate::model::Msg>,
        cfg: &crate::config::Config,
    ) -> Result<()> {
        msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        let dir = self.chats_dir().join(&meta.source).join(self.sanitize(&meta.project));
        std::fs::create_dir_all(&dir)?;
        let jsonl_path = dir.join(format!("{}.jsonl", crate::render::slug(&meta.id)));
        let md_path = dir.join(format!("{}.md", crate::render::slug(&meta.id)));

        let mut jsonl = String::new();
        jsonl.push_str(&serde_json::to_string(&serde_json::json!({
            "aiyou": "session",
            "source": meta.source,
            "id": meta.id,
            "project": meta.project,
            "title": meta.title,
            "created_ms": meta.created_ms,
            "updated_ms": meta.updated_ms,
        }))?);
        jsonl.push('\n');
        for m in msgs.iter_mut() {
            if cfg.redact_all {
                m.text = crate::render::redact(&m.text);
            }
            jsonl.push_str(&serde_json::to_string(m)?);
            jsonl.push('\n');
        }
        std::fs::write(&jsonl_path, jsonl)?;

        let md = if cfg.redact_markdown {
            let redacted: Vec<crate::model::Msg> = msgs
                .iter()
                .map(|m| crate::model::Msg { text: crate::render::redact(&m.text), ..m.clone() })
                .collect();
            crate::render::render_markdown(meta, &redacted)
        } else {
            crate::render::render_markdown(meta, msgs)
        };
        std::fs::write(&md_path, md)?;
        Ok(())
    }

    pub fn session_files(&self, source: &str, id: &str) -> (PathBuf, PathBuf) {
        let jsonl = self
            .find_session(source, id)
            .unwrap_or_else(|| self.chats_dir().join(source).join(format!("{}.jsonl", crate::render::slug(id))));
        let md = jsonl.with_extension("md");
        (jsonl, md)
    }

    pub fn read_session_jsonl(
        &self,
        source: &str,
        id: &str,
    ) -> Result<(crate::model::SessionMeta, Vec<crate::model::Msg>)> {
        let (jsonl_path, _) = self.session_files(source, id);
        self.read_session_jsonl_at(&jsonl_path)
    }

    pub fn read_session_jsonl_at(
        &self,
        jsonl_path: &Path,
    ) -> Result<(crate::model::SessionMeta, Vec<crate::model::Msg>)> {
        let raw = std::fs::read_to_string(jsonl_path)
            .with_context(|| format!("reading {}", jsonl_path.display()))?;
        let mut lines = raw.lines();
        let header: serde_json::Value =
            serde_json::from_str(lines.next().context("empty session file")?)
                .context("parsing session header")?;
        let meta = crate::model::SessionMeta {
            source: header["source"].as_str().unwrap_or_default().to_string(),
            id: header["id"].as_str().unwrap_or_default().to_string(),
            project: header["project"].as_str().unwrap_or_default().to_string(),
            title: header["title"].as_str().unwrap_or_default().to_string(),
            created_ms: header["created_ms"].as_i64().unwrap_or_default(),
            updated_ms: header["updated_ms"].as_i64().unwrap_or_default(),
        };
        let mut msgs = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            msgs.push(serde_json::from_str(line).with_context(|| "parsing message line")?);
        }
        Ok((meta, msgs))
    }

    /// Find the on-disk jsonl path for a session by scanning source dirs for `<slug>.jsonl`.
    pub fn find_session(&self, source: &str, id: &str) -> Option<PathBuf> {
        let slug = crate::render::slug(id);
        let src_dir = self.chats_dir().join(source);
        let Ok(entries) = std::fs::read_dir(&src_dir) else {
            return None;
        };
        for proj in entries.flatten() {
            let p = proj.path().join(format!("{slug}.jsonl"));
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }

    /// All stored sessions: (md_path, meta) sorted by (source, project, id).
    pub fn list_sessions(&self) -> Result<Vec<(PathBuf, crate::model::SessionMeta)>> {
        let mut out = Vec::new();
        if !self.chats_dir().exists() {
            return Ok(out);
        }
        for entry in walkdir::WalkDir::new(self.chats_dir())
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|e| e == "jsonl")
            {
                if let Ok((meta, _)) = self.read_session_jsonl_at(entry.path()) {
                    let md = entry.path().with_extension("md");
                    out.push((md, meta));
                }
            }
        }
        out.sort_by(|a, b| (&a.1.source, &a.1.project, &a.1.id).cmp(&(&b.1.source, &b.1.project, &b.1.id)));
        Ok(out)
    }
}
