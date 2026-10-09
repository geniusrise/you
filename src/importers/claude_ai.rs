use anyhow::Result;

pub fn parse_dump(_root: &std::path::Path, _exact: Option<&std::path::Path>, _source: &str) -> Result<Vec<(crate::model::SessionMeta, Vec<crate::model::Msg>)>> {
    anyhow::bail!("claude-ai import not implemented yet")
}
