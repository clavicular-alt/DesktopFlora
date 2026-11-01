# winget

The build fills these templates with each release's version and installer checksum (`VERSION`, `SHA256`) and keeps the
result with the build's files. With a repository secret `WINGET_TOKEN` (a GitHub token with the `public_repo` scope),
it also submits them to [winget-pkgs](https://github.com/microsoft/winget-pkgs), where Microsoft reviews them; once
accepted, `winget install clavicular-alt.DesktopFlora` installs DesktopFlora.
