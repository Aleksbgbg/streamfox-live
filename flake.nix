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
      in {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "backend";
          version = "0.0.0";

          src = nixpkgs.lib.cleanSource ./backend/.;
          cargoLock.lockFile = ./backend/Cargo.lock;
        };
      }
    );
  };
}
