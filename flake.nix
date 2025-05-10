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

    nixosModules.default = {
      config,
      lib,
      pkgs,
      ...
    }:
      with lib; let
        description = "WebRTC screen sharing server";
        cfg = config.services.streamfoxLive;
      in {
        options.services.streamfoxLive = {
          enable = mkEnableOption description;

          publicIp = mkOption {
            type = lib.types.str;
            description = "Public IP address to use for the WebRTC ICE host candidate";
          };

          portMin = mkOption {
            type = lib.types.ints.u16;
            description = "Minimum UDP port to use for WebRTC connections (inclusive)";
          };

          portMax = mkOption {
            type = lib.types.ints.u16;
            description = "Maximum UDP port to use for WebRTC connections (inclusive)";
          };
        };

        config = mkIf cfg.enable {
          systemd.services.streamfox-live = {
            inherit description;
            wantedBy = ["multi-user.target"];

            serviceConfig = {
              ExecStart =
                "${self.packages.${pkgs.system}.default}/bin/backend " +
                "--public-ip ${cfg.publicIp} " +
                "--port-min ${toString cfg.portMin} " +
                "--port-max ${toString cfg.portMax}";
              Restart = "always";
              Type = "exec";
            };
          };
        };
      };
  };
}
