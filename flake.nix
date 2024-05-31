{
  description = "suzuri";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };
  outputs =
    inputs@{
      flake-utils,
      crane,
      rust-overlay,
      nixpkgs,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.nightly.latest.default;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        watch-script =
          name: body:
          pkgs.writeShellScriptBin name ''
            while sleep 1; do
              echo "File list changed; restarting entr."
              echo "$(fd -p 'Cargo.toml|Cargo.lock|\.rs$' . -E 'legacy/' --no-ignore -E 'target/')" | \
                entr -scrd 'echo "$(date): files changed; rerunning command\n" && ${body}'
            done
          '';
      in
      {
        devShells.default = craneLib.devShell rec {
          name = "suzuri";
          LD_LIBRARY_PATH = "${nixpkgs.lib.strings.makeLibraryPath buildInputs}";
          buildInputs = with pkgs; [
            (watch-script "watch-ui" ''
              cargo build --release --bin szr_gpui
              patchelf --add-rpath ${pkgs.vulkan-loader}/lib target/release/szr_gpui
              echo "Patched RPATH"
              i3-msg "workspace 9:9"
              target/release/szr_gpui &
              sleep 0.5
              i3-msg "workspace 12:e"
            '')
            (watch-script "watch-run" ''
              cargo run --release --bin szr_web
            '')
            (watch-script "watch-test" ''
              sleep 0.2
              cargo nextest run --release
            '')
            alsa-lib
            cargo-nextest
            clang
            curl
            entr
            fontconfig
            freetype
            gcc
            libgit2
            libxkbcommon
            mold
            nixfmt-rfc-style
            nodejs
            nodePackages.npm
            nodePackages.sass
            openssl
            pgformatter
            pkg-config
            postgresql_16
            python3
            rlwrap
            rust-analyzer
            sqlite
            sqlx-cli
            tailwindcss
            taplo
            wayland
            xorg.libxcb
            zlib
            zstd
          ];
        };
      }
    );
}
