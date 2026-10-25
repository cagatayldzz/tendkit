
## Desktop downloads and support

Choose the download for your operating system and processor. This target matrix
covers mainstream desktop operating systems; it is not a measured 90% market
coverage claim and does not include phones, tablets, or ChromeOS.

| Operating system | Processor | Download | Status |
| --- | --- | --- | --- |
| macOS 15 or newer | Apple Silicon (M series) | `TendKit-VERSION-macos-arm64.zip` | Native app, signed and notarized |
| macOS 15 or newer | Intel 64-bit | `TendKit-VERSION-macos-x64.zip` | Native app, signed and notarized |
| Windows 10/11 | Intel/AMD 64-bit | `TendKit-VERSION-windows-x64.zip` | Native inspection preview; unsigned |
| Ubuntu 24.04 or newer | Intel/AMD 64-bit | `TendKit-VERSION-linux-x64.deb` | Native inspection preview |
| Linux with glibc 2.39 or newer | Intel/AMD 64-bit | `TendKit-VERSION-linux-x64.tar.gz` | Portable layout; system libraries required |

Windows and Linux previews provide tool discovery, environment inspection, and
disk usage indexing. Tool installation and application self-updates are currently
available on macOS only. Use the original tool manager and download application
updates manually on preview platforms. Other Linux distributions are not yet
validated; the tar archive is not a self-contained AppImage.

On macOS, extract the ZIP and move TendKit.app into Applications. On Windows,
extract the entire folder and run TendKit.exe; keep tendkit-engine.exe beside it.
On Ubuntu, install the Debian package through the system package installer.
For the Linux tar archive, extract it and run `TendKit/TendKit` in a graphical
X11 or Wayland session. The machine needs working graphics drivers and the
runtime libraries listed in the Debian package metadata.

`SHA256SUMS.txt` covers every download. Native builds and an engine protocol
smoke test must pass for every target before publishing. A GUI startup check and
manual testing on each operating system are still required before promoting
Windows or Linux from preview to full support.

Windows ARM64, Linux ARM64, 32-bit systems, Android, and iOS are not shipped.
Intel Macs require Homebrew to be installed separately through its official
installer; TendKit's bundled Homebrew package installer requires Apple Silicon.
