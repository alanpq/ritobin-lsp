{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";

    systems.url = "github:nix-systems/default";
  };

  outputs = {
    nixpkgs,
    systems,
    ...
  } @ inputs: let
    eachSystem = f:
      nixpkgs.lib.genAttrs (import systems) (
        system:
          f (import nixpkgs {
            inherit system;
            overlays = [inputs.rust-overlay.overlays.default];
          })
      );

    rustToolchain = eachSystem (pkgs: (pkgs.rust-bin.stable.latest.default.override {
      extensions = ["rust-src"];
    }));
  in {
    devShells = eachSystem (pkgs: {
      default = pkgs.mkShell {
        RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        packages = [
          rustToolchain.${pkgs.system}
          pkgs.rust-analyzer-unwrapped
          pkgs.cargo
          pkgs.cargo-insta
          pkgs.cargo-hack
          pkgs.bacon

          pkgs.nodejs

          # Alternatively, you can use a specific major version of Node.js

          # pkgs.nodejs-22_x

          # Use corepack to install npm/pnpm/yarn as specified in package.json
          pkgs.corepack

          # To install a specific alternative package manager directly,
          # comment out one of these to use an alternative package manager.

          # pkgs.yarn
          # pkgs.pnpm
          # pkgs.bun

          # Required to enable the language server
          pkgs.nodePackages.typescript
          pkgs.nodePackages.typescript-language-server

          # Python is required on NixOS if the dependencies require node-gyp

          # pkgs.python3
        ];
      };
    });
  };
}
