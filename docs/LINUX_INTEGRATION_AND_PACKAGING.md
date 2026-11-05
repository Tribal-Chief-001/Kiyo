# Linux Desktop Integration, Display Protocols, and Packaging

## 1. Linux Mint & Cinnamon First-Class Citizenship

Linux Mint (Cinnamon desktop environment) is a primary target platform for `Kiyo`. Rather than behaving like an alien, cross-platform abstraction that ignores desktop standards, `Kiyo` deeply integrates with Cinnamon's file manager (**Nemo**), window manager (**Muffin**), and system D-Bus services.

```
+-----------------------------------------------------------------------------------------+
|                               Cinnamon Desktop Shell (Mint)                             |
+-----------------------------------------------------------------------------------------+
| [Nemo Context Actions]  ~/.local/share/nemo/actions/kiyo.nemo_action                    |
| [Desktop Entry]         /usr/share/applications/kiyo.desktop                            |
| [D-Bus Appearance]      org.freedesktop.appearance.color-scheme (Live Theme Switching)  |
| [X11 / Wayland Bridge]  x11rb 0.13 PRIMARY Selection + wp_fractional_scale_v1           |
| [Portals Sandbox]       ashpd 0.13 (Native File Pickers & Printing via Flatpak/Host)    |
+-----------------------------------------------------------------------------------------+
```

### 1.1. Deep Nemo Context Actions (`kiyo.nemo_action`)
In Linux Mint, Nemo supports rich contextual action definitions that appear on right-clicking files. `Kiyo` provides native context actions enabling instant document operations without opening a terminal:

```ini
# ~/.local/share/nemo/actions/kiyo.nemo_action

[Nemo Action]
Active=true
Name=Kiyo PDF Actions
Comment=High-performance PDF viewing, OCR, diffing, and merging
Exec=<kiyo-action.sh %F>
Icon-Name=application-pdf
Selection=any
Extensions=pdf;PDF;
Quote=double

[Action Root]
Name=Kiyo Document Operations
Icon-Name=application-pdf

[Action item_open]
Name=Open in Kiyo (GPU Accelerated)
Exec=kiyo gui %F

[Action item_ocr]
Name=OCR: Convert to Searchable PDF
Exec=kiyo ocr --sandwich %F
Conditions=mimetype=application/pdf;

[Action item_diff]
Name=Visual Diff Revisions with Kiyo
Exec=kiyo diff %F
Conditions=selection_count=2;mimetype=application/pdf;

[Action item_merge]
Name=Merge Selected PDFs into Single Document
Exec=kiyo merge %F --output "merged_output.pdf"
Conditions=selection_count=>1;mimetype=application/pdf;
```

### 1.2. Desktop Entry Specification (`kiyo.desktop`)
```ini
[Desktop Entry]
Name=Kiyo
GenericName=PDF Viewer & Manipulator
Comment=High-framerate GPU-accelerated PDF viewer and document engine
Exec=kiyo gui %U
Terminal=false
Type=Application
Icon=org.kiyo.Kiyo
Categories=Office;Viewer;Graphics;Publishing;
MimeType=application/pdf;application/x-pdf;application/x-bzpdf;application/x-gzpdf;
Keywords=PDF;Viewer;Document;OCR;Diff;GPU;Vim;
Actions=DiffMode;

[Desktop Action DiffMode]
Name=Compare Two PDFs
Exec=kiyo diff %U
```

---

## 2. Wayland Fractional Scaling and HiDPI Fidelity

### 2.1. The Fractional Scaling Blurriness Trap
On modern displays (e.g. 13-inch 2560x1600 or 27-inch 4K panels), Linux users often run fractional scaling factors like **125%** ($1.25\times$) or **150%** ($1.5\times$). Traditional PDF viewers (Poppler/GTK3/Xreader) suffer from a critical flaw:
- They render the page bitmap at $1.0\times$ logical scale.
- The Wayland compositor then upscales the raster buffer using bilinear or bicubic filtering.
- Result: Fine mathematical fonts, vector line diagrams, and small text appear fuzzy and blurry.

### 2.2. Kiyo's Exact Physical Pixel Scale Architecture
`Kiyo` leverages the Wayland `wp_fractional_scale_v1` and `wp_viewporter` protocols:
1. `Kiyo` queries the exact fractional scale from the Wayland compositor (e.g. $125 / 120 = 1.25$).
2. Rather than scaling logical coordinates, `Kiyo` multiplies its tile rendering viewport matrix by the exact physical device pixel ratio:
   $$\text{RasterScale} = \text{ZoomFactor} \times \text{CompositorScale}$$
3. Tiles are rasterized directly at physical display pixel resolution and presented to the Wayland surface via `wp_viewport.set_destination`.
4. Result: Zero bilinear blur; crisp typography on every fractional scale.

```rust
// crates/kiyo-gui/src/scaling.rs

pub struct ViewportScaleManager {
    pub logical_zoom: f32,
    pub compositor_fractional_scale: f32, // e.g. 1.25 for 125%
}

impl ViewportScaleManager {
    /// Computes exact target pixel dimensions for rasterization
    pub fn target_raster_pixels(&self, pdf_point_width: f32, pdf_point_height: f32) -> (u32, u32) {
        let physical_dpi_scale = (self.compositor_fractional_scale * 72.0) / 72.0;
        let effective_scale = self.logical_zoom * physical_dpi_scale;
        
        let width_px = (pdf_point_width * effective_scale).ceil() as u32;
        let height_px = (pdf_point_height * effective_scale).ceil() as u32;
        (width_px, height_px)
    }
}
```

---

## 3. D-Bus Integration and Instant Theme Detection

Linux Mint Cinnamon, GNOME, and KDE emit appearance change events over the system D-Bus bus following the FreeDesktop XDG Desktop Portal Appearance standard:
- Interface: `org.freedesktop.portal.Settings`
- Namespace: `org.freedesktop.appearance`
- Key: `color-scheme` (`0` = Default/No Preference, `1` = Prefer Dark, `2` = Prefer Light)

`Kiyo` listens to D-Bus signals asynchronously using `zbus 5.5`:
- When the user toggles Dark Mode in Linux Mint Cinnamon System Settings, `Kiyo` receives the signal in < 1 millisecond.
- It updates the GPU uniform buffer `uniforms.invert_images` and triggers a single frame presentation pass.
- **Zero Restart Required**: The document switches seamlessly from Light to Smart Dark Mode without interrupting the user.

```rust
// crates/kiyo-gui/src/theme.rs

use zbus::{Connection, Result};
use zbus::proxy;

#[proxy(
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop",
    interface = "org.freedesktop.portal.Settings"
)]
trait DesktopSettings {
    fn read(&self, namespace: &str, key: &str) -> Result<zbus::zvariant::OwnedValue>;
}

pub async fn watch_system_theme_stream(tx: tokio::sync::mpsc::Sender<bool>) -> Result<()> {
    let connection = Connection::session().await?;
    let proxy = DesktopSettingsProxy::new(&connection).await?;

    let value = proxy.read("org.freedesktop.appearance", "color-scheme").await?;
    let is_dark = parse_color_scheme(value);
    let _ = tx.send(is_dark).await;

    // Listen to SettingChanged signals for live toggle
    // ...
    Ok(())
}
```

---

## 4. X11 Clipboard and `PRIMARY` Selection (`x11rb`)

On Linux, power users rely heavily on the **`PRIMARY` selection buffer**: selecting text with the mouse instantly copies it, and pressing the middle mouse button in a terminal pastes it immediately, without pressing `Ctrl+C`.

Most modern GUI toolkits (especially non-GTK engines) break this contract. `Kiyo` implements native X11 `PRIMARY` selection management using `x11rb 0.13`:

1. When the user finishes dragging a selection rectangle in `kiyo-gui`, `Kiyo` asserts ownership of the `XA_PRIMARY` atom via `x11rb::protocol::xproto::set_selection_owner`.
2. When a target window (e.g. Kitty, Alacritty, or Bash) requests the selection, `Kiyo` handles `SelectionRequest` events and serves `UTF8_STRING` formats over the X11 connection.
3. On Wayland, the identical behavior is achieved via the `zwp_primary_selection_device_manager_v1` protocol.

---

## 5. Universal Desktop Portals (`ashpd 0.13`)

For sandboxed packaging (Flatpak) and hermetic host compatibility, `Kiyo` uses `ashpd 0.13`:
- **File Pickers**: Invokes native Cinnamon/GTK/KDE file open and file save dialogs via `ashpd::desktop::file_chooser::FileChooser`.
- **System Printing**: Forwards PDF print spool requests directly to CUPS via `ashpd::desktop::print::Print`.
- **Notification Daemon**: Signals completion of long batch operations (e.g. OCR across a 500-page bundle) via native desktop notifications.

---

## 6. Hermetic Distribution and Packaging Matrix

`Kiyo` is packaged across three primary channels to guarantee effortless installation on Linux Mint, Ubuntu, Debian, Arch, and Fedora:

```
+-----------------------------------------------------------------------------------------+
|                                  Distribution Targets                                   |
+-----------------------------------------------------------------------------------------+
| 1. Native Debian / Mint (.deb) | apt install ./kiyo_0.1.0_amd64.deb                     |
|                                | Includes Nemo action, desktop file, and man pages      |
+--------------------------------+--------------------------------------------------------+
| 2. Universal AppImage (.AppImage)| Single executable binary; runs on any Linux distro    |
|                                | Bundles all graphic backends and offline OCR weights   |
+--------------------------------+--------------------------------------------------------+
| 3. Flathub Flatpak             | org.kiyo.Kiyo; sandboxed with portal permissions       |
+-----------------------------------------------------------------------------------------+
```

### 6.1. Flatpak Manifest (`org.kiyo.Kiyo.json`)
```json
{
  "app-id": "org.kiyo.Kiyo",
  "runtime": "org.freedesktop.Platform",
  "runtime-version": "24.08",
  "sdk": "org.freedesktop.Sdk",
  "sdk-extensions": ["org.freedesktop.Sdk.Extension.rust-stable"],
  "command": "kiyo",
  "finish-args": [
    "--share=ipc",
    "--socket=x11",
    "--socket=wayland",
    "--device=dri",
    "--filesystem=host",
    "--talk-name=org.freedesktop.portal.Settings"
  ],
  "modules": [
    {
      "name": "kiyo",
      "buildsystem": "simple",
      "build-commands": [
        "cargo build --release",
        "install -Dm755 target/release/kiyo /app/bin/kiyo",
        "install -Dm644 packaging/org.kiyo.Kiyo.desktop /app/share/applications/org.kiyo.Kiyo.desktop",
        "install -Dm644 packaging/org.kiyo.Kiyo.svg /app/share/icons/hicolor/scalable/apps/org.kiyo.Kiyo.svg"
      ]
    }
  ]
}
```

### 6.2. AppImage Recipe (`AppImageBuilder.yml`)
```yaml
version: 1
AppDir:
  path: ./AppDir
  app_info:
    id: org.kiyo.Kiyo
    name: Kiyo
    icon: org.kiyo.Kiyo
    version: 0.1.0
    exec: usr/bin/kiyo
    exec_args: "$@"
  apt:
    arch: amd64
    sources:
      - sourceline: "deb http://archive.ubuntu.com/ubuntu/ noble main universe"
    include:
      - libvulkan1
      - libx11-6
      - libxfixes3
  files:
    include:
      - target/release/kiyo: usr/bin/kiyo
```
