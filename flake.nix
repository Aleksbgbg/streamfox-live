{
  description = "Streamfox Live";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05";
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
      utils,
      ...
    }:
      with lib; let
        description = "WebRTC screen sharing server";
        cfg = config.services.streamfoxLive;
      in {
        options.services.streamfoxLive = {
          enable = mkEnableOption description;

          publicIp = mkOption {
            type = types.str;
            description = "Public IP address to use for the WebRTC ICE host candidate";
          };

          webRtcPortMux = mkOption {
            type = types.nullOr types.ints.u16;
            default = null;
            description = "Multiplex all WebRTC connections on the specified UDP port";
          };

          webRtcPortMin = mkOption {
            type = types.nullOr types.ints.u16;
            default = null;
            description = "Minimum UDP port to use for WebRTC connections (inclusive)";
          };

          webRtcPortMax = mkOption {
            type = types.nullOr types.ints.u16;
            default = null;
            description = "Maximum UDP port to use for WebRTC connections (inclusive)";
          };

          debug.webRtcLogLevel = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = ''
              Emit webrtc-rs logs that are at the specified verbosity or lower [to journald]
            '';
          };
        };

        config = mkIf cfg.enable {
          systemd.services.streamfox-live = {
            inherit description;
            wantedBy = ["multi-user.target"];

            serviceConfig = {
              ExecStart = utils.escapeSystemdExecArgs (
                [
                  "${self.packages.${pkgs.system}.default}/bin/backend"
                  "--public-ip"
                  cfg.publicIp
                ]
                ++ lists.flatten
                (
                  lists.optional
                  (cfg.webRtcPortMux != null)
                  ["--webrtc-port-mux" cfg.webRtcPortMux]
                )
                ++ lists.flatten
                (
                  lists.optional
                  ((cfg.webRtcPortMin != null) && (cfg.webRtcPortMax != null))
                  ["--webrtc-port-min" cfg.webRtcPortMin "--webrtc-port-max" cfg.webRtcPortMax]
                )
                ++ lists.flatten
                (
                  lists.optional
                  (cfg.debug.webRtcLogLevel != null)
                  ["--webrtc-log-level" cfg.debug.webRtcLogLevel]
                )
              );
              Restart = "always";
              Type = "exec";
            };
          };
        };
      };
  };
}
