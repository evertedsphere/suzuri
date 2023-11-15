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
          pkgs.mkShell {
            name = "suzuri";
            packages = with pkgs; [
              pkg-config
              gcc
              rustup
              cargo-watch
              trunk
              wasm-bindgen-cli
              nodePackages.sass
              nodePackages.npm
              rust-analyzer
              nodejs
              librsvg
              webkitgtk
              libsoup
              postgresql_15
              rlwrap
              entr
              openssl
              cargo-generate
              nodePackages.typescript-language-server
              #nodePackages.typescript-svelte-plugin
              nodePackages.svelte-language-server
            ];
          };
      });
}
