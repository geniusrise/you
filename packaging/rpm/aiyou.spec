Name:           aiyou
Version:        REPLACE_VERSION
Release:        1%{?dist}
Summary:        Export, maintain and analyze all your AI coding-agent chat history
License:        MIT
URL:            https://github.com/geniusrise/you
Source0:        aiyou
BuildArch:      REPLACE_ARCH
AutoReqProv:    no
Requires:       git

%description
Export, maintain and analyze all your AI coding-agent chat history — Claude
Code, OpenCode, Codex, Charm Crush, Gemini CLI, Pi, Antigravity, Goose,
Copilot, Cursor, Amp, Continue, Zed and more — in one normalized,
git-versioned store on disk. Import ChatGPT, Claude.ai and Gemini dumps
alongside them, then run an LLM swarm over everything to distill your
personality, preferences and workflow into a portable SKILL.md.

%prep
%setup -q -n aiyou-binary

%build
# prebuilt binary supplied by the release workflow

%install
install -Dm755 aiyou %{buildroot}%{_bindir}/aiyou
install -Dm644 README.md %{buildroot}%{_docdir}/aiyou/README.md
install -Dm644 LICENSE %{buildroot}%{_docdir}/aiyou/LICENSE
install -Dm644 aiyou.bash %{buildroot}%{_datadir}/bash-completion/completions/aiyou
install -Dm644 _aiyou %{buildroot}%{_datadir}/zsh/site-functions/_aiyou
install -Dm644 aiyou.fish %{buildroot}%{_datadir}/fish/vendor_completions.d/aiyou.fish

%files
%{_bindir}/aiyou
%{_docdir}/aiyou/README.md
%{_docdir}/aiyou/LICENSE
%{_datadir}/bash-completion/completions/aiyou
%{_datadir}/zsh/site-functions/_aiyou
%{_datadir}/fish/vendor_completions.d/aiyou.fish
