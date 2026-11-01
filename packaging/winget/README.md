# winget

Templates for submitting DesktopFlora to [winget-pkgs](https://github.com/microsoft/winget-pkgs).
For each release, replace `VERSION` and `SHA256` (from the release's `SHA256SUMS.txt`, in upper case),
then open a pull request at microsoft/winget-pkgs under `manifests/c/clavicular-alt/DesktopFlora/VERSION/`,
or let `wingetcreate update clavicular-alt.DesktopFlora --version VERSION --urls <installer url> --submit` do it.
