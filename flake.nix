{
  description = "n8n CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "n8n";
          version = "0.1.0";
          src = self;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          meta = {
            mainProgram = "n8n";
            description = "n8n CLI";
            homepage = "https://github.com/radjathaher/n8n-cli";
            license = pkgs.lib.licenses.mit;
          };
        };
      }
    );
}
