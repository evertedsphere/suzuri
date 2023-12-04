{
  description = "suzuri";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = inputs@{ flake-utils, nixpkgs, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };

        watch-script = name: body:
          pkgs.writeShellScriptBin name ''
            while sleep 1; do
              echo "File list changed; restarting entr."
              echo "$(fd -p 'Cargo.toml|Cargo.lock|\.rs$' . -E 'legacy/' --no-ignore -E 'target/')" | \
                entr -scrd 'echo "$(date): files changed; rerunning command\n" && ${body}'
            done
          '';
      in {
        devShells.default = pkgs.mkShell rec {
          name = "suzuri";
          LD_LIBRARY_PATH =
            "${nixpkgs.lib.strings.makeLibraryPath buildInputs}";
          buildInputs = with pkgs; [
            (watch-script "watch-run" ''
              cargo run --release
            '')
            (watch-script "watch-test" ''
              sleep 0.2
              cargo nextest run --release
            '')
            # cargo-generate
            # cmake
            entr
            cargo-nextest
            diesel-cli
            # fontconfig
            gcc
            # gettext
            # libGL
            # librsvg
            # libsoup
            nixfmt
            nodejs
            nodePackages.npm
            nodePackages.sass
            openssl
            pkg-config
            postgresql_15
            python3
            rlwrap
            rust-analyzer
            rustc
            cargo
            # rustup
            taplo
            # vulkan-headers
            # vulkan-loader
            # vulkan-tools
            # wasm-bindgen-cli
            # webkitgtk
            # xorg.libX11
            # xorg.libXcursor
            # xorg.libXi
            # xorg.libXrandr
          ];
        };
      });
}
