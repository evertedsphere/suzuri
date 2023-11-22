{
  description = "Reader app";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = inputs@{ flake-utils, nixpkgs, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { inherit system; config.allowUnfree = true; };
      in
      {
        devShells.default =
          pkgs.mkShell rec {
            name = "suzuri";
          LD_LIBRARY_PATH="${nixpkgs.lib.strings.makeLibraryPath buildInputs}";
            buildInputs = with pkgs; [
              cargo-generate
              cargo-watch
              cmake
              entr
              fontconfig
              gcc
              libGL
              librsvg
              libsoup
              nodejs
              nodePackages.npm
              nodePackages.sass
              nodePackages.svelte-language-server
              nodePackages.typescript-language-server
              #nodePackages.typescript-svelte-plugin
              openssl
              pkg-config
              postgresql_15
              rlwrap
              rust-analyzer
              rustup
              trunk
              vulkan-headers
              vulkan-loader
              vulkan-tools
              wasm-bindgen-cli
              webkitgtk
              xorg.libX11
              xorg.libXcursor
              xorg.libXi
              xorg.libXrandr
            ];
          };
      });
}
