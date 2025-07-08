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
        runtimeFilesDir = "/var/run/streamfox-live";
        socketPath = "${runtimeFilesDir}/http.sock";
      in {
        options.services.streamfoxLive = {
          enable = mkEnableOption description;

          publicIp = mkOption {
            type = types.str;
            description = "Public IP address to use for the WebRTC ICE host candidate";
          };

          httpPort = mkOption {
            type = types.nullOr types.ints.u16;
            default = null;
            description = "Accept HTTP requests on the specified TCP port";
          };

          httpUnixSocket = mkOption {
            type = types.bool;
            default = false;
            description = ''
              Whether to accept HTTP requests over a unix socket (located at ${socketPath})
            '';
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
          users.groups."streamfox-live" = {};
          users.users."streamfox-live" = {
            group = "streamfox-live";
            isSystemUser = true;
          };

          systemd = {
            services.streamfox-live = {
              inherit description;
              wantedBy = ["multi-user.target"];

              serviceConfig = {
                ExecStart = utils.escapeSystemdExecArgs (
                  [
                    "${self.packages.${pkgs.system}.default}/bin/backend"
                    "--public-ip"
                    cfg.publicIp
                  ]
                  ++ (
                    if cfg.httpUnixSocket
                    then ["--http-unix-socket" socketPath]
                    else ["--http-port" cfg.httpPort]
                  )
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

                User = "streamfox-live";
                Group = "streamfox-live";

                Restart = "always";
                Type = "exec";
              };
            };

            tmpfiles.rules = mkIf cfg.httpUnixSocket [
              # Type Path Mode User Group Age Argument
              "d ${runtimeFilesDir} 0755 streamfox-live streamfox-live - -"
            ];
          };
        };
      };
  };
}
