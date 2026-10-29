%define debug_package %{nil}

Name:           aiyou
Version:        REPLACE_VERSION
Release:        1%{?dist}
Summary:        Teach every AI agent who you are: your personality and working style, extracted from your chats
License:        MIT
URL:            https://github.com/geniusrise/you
Source0:        aiyou-binary.tar.gz
BuildArch:      REPLACE_ARCH
AutoReqProv:    no
Requires:       git

%description
aiyou reads your real conversations across Claude Code, OpenCode, Codex,
Cursor, Copilot, Gemini CLI and more (plus ChatGPT, Claude.ai and Gemini
dumps), extracts your personality, taste and working style into a portable
'me' skill, and keeps it current as you change. Every update is proposed
first; nothing is applied until you review and accept it. Local-first, in a
git-versioned store.

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
