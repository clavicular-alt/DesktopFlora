# DesktopFlora

Schnee (oder Regen, Blüten, Laub) auf dem Windows-Desktop: fällt sanft, bleibt auf Fenstern liegen, rutscht beim Verschieben ab und lässt sich mit dem Mauszeiger wegfegen.

Basiert auf [Snowhush](https://github.com/mbezruchko/Snowhush) von Mykhailo Bezruchko (MIT-Lizenz) und erweitert dessen Windows-Version. Hieß vorher „Snowhush Plus“; eine vorhandene Installation wird beim Installieren ersetzt, die Einstellungen bleiben.

**Download:** [Neuestes Release](https://github.com/clavicular-alt/DesktopFlora/releases/latest) → `DesktopFlora-Setup.exe`. Installiert nur für den aktuellen Benutzer, ohne Adminrechte.

## Neu gegenüber dem Original

| Funktion | Wo im Tray-Menü |
|---|---|
| **Wetterarten:** Schnee, Regen (Streifen, bleibt nicht liegen), Kirschblüten, Herbstlaub (taumelt, bunte Haufen) | Weather |
| **Echtes Wetter:** fällt nur, was draußen gerade fällt, inkl. Stärke und Wind (Open-Meteo, Standort von Windows, auf ~1 km gerundet, alle 30 min) | Weather → Match Local Weather |
| **Flockengröße, Wind, Deckkraft** | Look |
| **Bildrate begrenzen** (Automatisch / 60 / 30 fps) für weniger CPU | Look → Frame Rate |
| **Beim Tippen ausblenden:** Schnee wird blass, solange getippt wird (ohne Tastatur-Hook: erkennt Eingaben bei ruhender Maus) | Look → Fade While Typing |
| **Schneefreie Programme:** kein Schnee auf deren Fenstern; maximiert und im Vordergrund verschwindet der Schnee auf dem Bildschirm ganz | Snow-Free Programs |
| **Updates in der App:** lädt das neue Setup, prüft den SHA-256 gegen den Release (GitHub-Digest oder `SHA256SUMS.txt`), installiert still und startet neu. Ohne passende Prüfsumme wird nichts ausgeführt | Check for Updates… |
| **Schneebälle:** fliegen aus dem Schneehaufen zum Mauszeiger und zerplatzen | Strg+Alt+B, Spaß → Schneeball werfen |
| **Schneemänner:** aus dem Schnee am Boden gebaut (bis zu 4), schmelzen mit, fallen um, wenn man mit der Maus hineinfährt | Strg+Alt+N, Spaß → Schneemann bauen |
| **Wind von der Maus:** schnelle Bewegungen wirbeln die Flocken auf (Aus / Sanft / Stark) | Aussehen → Wind von der Maus |
| **Regentropfen spritzen** auf Fenstern und am Boden | Wetter → Regen |
| **Nach Jahreszeit:** Winter Schnee, Frühling Blüten, Sommer Regen, Herbst Laub | Wetter → Nach Jahreszeit |
| **Farben** (Eisblau, Gold, Rosa, Mint, Lavendel) und **Glitzern** im Schnee | Aussehen |
| **Zeitplan:** nur abends, tagsüber oder nachts | Zeitplan |
| **Tastenkürzel:** Strg+Alt+S an/aus, C wegfegen, W nächstes Wetter, B Schneeball, M Schneemann; **Doppelklick** aufs Symbol: an/aus | Tastenkürzel |
| **Deutsches Menü**, wenn Windows auf Deutsch läuft | – |
| **Sparsamer:** niedrige Priorität und Effizienzmodus (Windows 11), Interaktion höchstens 60 fps, 4 fps wenn nichts zu sehen ist, Sparmodus 20 fps, Speicher wird regelmäßig freigegeben | Aussehen → Bildrate |
| **Autostart:** startet ab sofort mit Windows (einmalig eingeschaltet, im Menü abschaltbar) | Beim Anmelden starten |
| **Signierte Builds** möglich (siehe unten) und **winget**-Vorlagen in `packaging/winget` | – |

Unverändert vom Original: Pause bei Vollbild-Apps, Präsentationen, Sperrbildschirm und ausgeschaltetem Display; ein Overlay pro Bildschirm; weniger Leistung im Akku- und Energiesparmodus; unsichtbar in Screenshots.

## Bauen

GitHub Actions baut bei jedem Push auf `main` `DesktopFlora-Setup.exe` und veröffentlicht sie als Release `v<Version>` (Version aus `windows/Cargo.toml`) mit `SHA256SUMS.txt`. Für ein Update: Version erhöhen, pushen.

Lokal unter Windows (Rust stable):

```powershell
cargo test --manifest-path core/Cargo.toml
cargo build --release --manifest-path windows/Cargo.toml --bin DesktopFlora
$env:SNOWHUSH_APP = "$PWD\windows\target\release\DesktopFlora.exe"
cargo build --release --manifest-path windows/Cargo.toml --bin DesktopFlora-Setup --features setup
```

Von macOS/Linux aus: `windows/build.sh` (braucht `cargo-xwin`).

## Signieren (gegen Virenscanner-Fehlalarme)

Ohne Signatur melden einzelne Scanner und SmartScreen das Programm oft fälschlich. Für signierte Builds:

1. Code-Signing-Zertifikat besorgen (z. B. kostenlos für Open Source über [SignPath Foundation](https://signpath.org), oder kommerziell).
2. Im Repo unter *Settings → Secrets and variables → Actions* anlegen:
   - `SIGN_CERT_BASE64`: die `.pfx`-Datei als Base64 (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`)
   - `SIGN_PASSWORD`: ihr Passwort
3. Der Workflow signiert dann App und Installer automatisch.

## Datenschutz

- Netzwerk nur für: Update-Check (`api.github.com`, `github.com`, nur dieses Repo) und, wenn eingeschaltet, Wetter (`api.open-meteo.com`, mit auf 2 Nachkommastellen gerundeten Koordinaten).
- Keine Telemetrie, kein Tastatur-Hook, keine Fenstertitel oder -inhalte.

## Lizenz

MIT, siehe [LICENSE](LICENSE).
