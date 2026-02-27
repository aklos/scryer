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

    # Tauri system deps
    pkg-config
    openssl
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
  '';
}
