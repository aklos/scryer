{ pkgs ? import <nixpkgs> {} }:

let
  # Copilot CLI, for exercising scryer's Copilot integration (hook payloads, the
  # ACP handshake) against the real binary rather than the docs. It is an unfree
  # package, so it joins the shell only where unfree is already permitted —
  # either in the caller's nixpkgs config or via NIXPKGS_ALLOW_UNFREE=1. Nobody's
  # license stance gets decided for them, and a contributor who hasn't opted in
  # still gets a working shell, just without the Copilot integration tests.
  # Claude Code and Codex are installed out-of-band; only this one is packaged.
  unfreeOk = (pkgs.config.allowUnfree or false) || builtins.getEnv "NIXPKGS_ALLOW_UNFREE" == "1";
  copilot = pkgs.lib.optional unfreeOk pkgs.github-copilot-cli;
in
pkgs.mkShell {
  buildInputs = (with pkgs; [
    # Node
    nodejs
    pnpm

    # Rust
    rustc
    cargo
    rustfmt
    clippy
    # Test runner that emits JUnit XML (the `junit` profile in
    # .config/nextest.toml), which `ingest_test_report` reads to record
    # per-claim verdicts. Plain `cargo test` emits no machine-readable report,
    # so without this the model's test lane can never be filled in.
    cargo-nextest

    # Image tools
    imagemagick

    # Demo / trailer capture (Playwright drives this chromium, CDP screencast
    # captures the frames, ffmpeg encodes)
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
  ]) ++ copilot;

  shellHook = ''
    export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
    export GSETTINGS_SCHEMA_DIR="${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}/glib-2.0/schemas"

    # Playwright's prebuilt browsers are FHS binaries that fail on NixOS
    # (missing libglib etc). Skip the download and drive the Nix-built chromium.
    export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
    export PLAYWRIGHT_CHROMIUM_EXECUTABLE="${pkgs.chromium}/bin/chromium"
${pkgs.lib.optionalString (!unfreeOk) ''
    echo "note: copilot (unfree) left out of this shell — re-enter with NIXPKGS_ALLOW_UNFREE=1 to test the Copilot integration."
''}  '';
}
