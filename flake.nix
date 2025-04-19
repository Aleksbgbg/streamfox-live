{
  description = "Streamfox Live";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    supportedSystems = ["x86_64-linux"];
    forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
  in {
    packages = forAllSystems (
      system: let
        pkgs = nixpkgs.legacyPackages.${system};
        frontend = pkgs.buildNpmPackage {
          pname = "frontend";
          version = "0.0.0";

          src = ./frontend/.;
          npmDeps = pkgs.importNpmLock {
            npmRoot = ./frontend/.;
          };

          npmConfigHook = pkgs.importNpmLock.npmConfigHook;
        };
      in {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "backend";
          version = "0.0.0";

          src = nixpkgs.lib.cleanSource ./backend/.;
          cargoLock.lockFile = ./backend/Cargo.lock;

          postInstall = ''
            cp -r ${frontend}/lib/node_modules/frontend/dist $out/bin/frontend
          '';
        };
      }
    );
  };
}
