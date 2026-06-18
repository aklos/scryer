{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Node
    nodejs
    pnpm

    # Rust
    rustc
    cargo
    rustfmt
    clippy

    # Image tools
    imagemagick

    # Demo / trailer capture (Playwright drives this chromium; ffmpeg encodes)
    chromium
    ffmpeg

    # Tauri system deps
    pkg-config
    openssl
    zlib
    glib
    gtk3
    libsoup_3
    webkitgtk_4_1
    cairo
    pango
    gdk-pixbuf
    atk
  ];

  shellHook = ''
    export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
    export GSETTINGS_SCHEMA_DIR="${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}/glib-2.0/schemas"

    # Playwright's prebuilt browsers are FHS binaries that fail on NixOS
    # (missing libglib etc). Skip the download and drive the Nix-built chromium.
    export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
    export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.chromium}/bin/chromium"
  '';
}
