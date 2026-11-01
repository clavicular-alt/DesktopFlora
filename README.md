# DesktopFlora

Schnee (oder Regen, Blüten, Laub) auf dem Windows-Desktop: fällt sanft, bleibt auf Fenstern liegen, rutscht beim Verschieben ab und lässt sich mit dem Mauszeiger wegfegen.

Basiert auf [Snowhush](https://github.com/mbezruchko/Snowhush) von Mykhailo Bezruchko (MIT-Lizenz) und erweitert dessen Windows-Version. Hieß vorher „Snowhush Plus“; eine vorhandene Installation wird beim Installieren ersetzt, die Einstellungen bleiben.

**Download:** [Neuestes Release](https://github.com/clavicular-alt/DesktopFlora/releases/latest) → `DesktopFlora-Setup.exe`. Installiert nur für den aktuellen Benutzer, ohne Adminrechte.

## Neu gegenüber dem Original

| Funktion | Wo |
|---|---|
| **Eigenes Einstellungsfenster:** Links- oder Rechtsklick aufs Symbol öffnet ein schlichtes Schwarz-Weiß-Fenster mit Milchglas (Windows 11) und vier Reitern (Wetter, Aussehen, Verhalten, Mehr), hell oder dunkel wie Windows | Symbol im Infobereich |
| **An / Aus:** Schalter oben im Fenster, Doppelklick aufs Symbol oder Tastenkürzel | – |
| **Wetterarten:** Schnee, Regen (spritzt auf), Blüten, Laub (taumelt, bunte Haufen), Sterne (funkeln) | Wetter |
| **Jahreszeit:** Winter Schnee, Frühling Blüten, Sommer Sterne, Herbst Laub, passend zum Land aus den Windows-Regionseinstellungen (Südhalbkugel umgekehrt) | Wetter → Jahreszeit |
| **Echtes Wetter vor Ort:** Regen oder Schnee wie draußen, inkl. Stärke und Wind; bei trockenem Wetter die Jahreszeit am Standort (Open-Meteo, Standort auf ~1 km gerundet, alle 30 min) | Wetter → Echtes Wetter vor Ort |
| **Tastenkürzel frei wählbar:** An/Aus, Wegfegen, Nächstes Wetter; belegte Kürzel werden angezeigt | Mehr → Tastenkürzel |
| **Ruhig im Hintergrund:** normale Priorität beim Zeichnen (kein Ruckeln), Effizienzmodus und Speicherfreigabe nur, wenn nichts zu sehen ist; standardmäßig 30 fps, 4 fps ohne Schnee, Sparmodus 20 fps, Fensterliste nur einmal pro Sekunde nachgeprüft | Verhalten → Bildrate |
| **Größe, Wind, Deckkraft, Glitzern** | Aussehen |
| **Beim Tippen ausblenden:** Schnee wird blass, solange getippt wird (ohne Tastatur- oder Maus-Hook: erkennt eine Reihe von Eingaben bei ruhender Maus) | Verhalten |
| **Schneefreie Programme:** kein Schnee auf deren Fenstern; maximiert und im Vordergrund verschwindet der Schnee auf dem Bildschirm ganz | Mehr |
| **Updates in der App:** lädt das neue Setup, prüft den SHA-256 gegen den Release (GitHub-Digest oder `SHA256SUMS.txt`), installiert still und startet neu. Ohne passende Prüfsumme wird nichts ausgeführt | Mehr → Nach Updates suchen |
| **Wind von der Maus:** schnelle Bewegungen wirbeln die Flocken auf (Aus / Sanft / Stark) | Aussehen → Wind von der Maus |
| **Farben:** natürlich oder bis zu 5 eigene Farben aus einer Farbleiste | Aussehen → Farbe |
| **Zeitplan:** nur abends, tagsüber oder nachts | Verhalten → Zeitplan |
| **Deutsch**, wenn Windows auf Deutsch läuft | – |
| **Autostart:** startet ab sofort mit Windows (einmalig eingeschaltet, abschaltbar) | Verhalten → Mit Windows starten |
| **Signierte Builds** möglich (siehe unten) und **winget**: wird bei jedem Release automatisch eingereicht, sobald das Secret `WINGET_TOKEN` gesetzt ist (siehe `packaging/winget`) | – |

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
