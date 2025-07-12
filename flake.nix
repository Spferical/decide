{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-25.05";

    crane = {
      url = "github:ipetkov/crane";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, advisory-db, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
        };
        inherit (pkgs) lib;

        craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
        src = lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type: (craneLib.filterCargoSources path type) || (builtins.match ".*/migrations/.*$" path != null);
        };

        commonArgs = {
          inherit src;
          pname = "decide";
          version = "0.1.0";
          strictDeps = true;
          buildInputs = [ ] ++ lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];
        };

        craneLibLLvmTools = craneLib.overrideToolchain (fenix.packages.${system}.complete.withComponents [ "cargo" "llvm-tools" "rustc" ]);
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        decide = craneLib.buildPackage ({ inherit cargoArtifacts; } // commonArgs);

      in
      {
        checks = {
          inherit decide;

          decide-clippy = craneLib.cargoClippy ({ cargoClippyExtraArgs = "--all-targets -- --deny warnings"; inherit cargoArtifacts; } // commonArgs);
          decide-doc = craneLib.cargoDoc ({ inherit cargoArtifacts; } // commonArgs);
          decide-fmt = craneLib.cargoFmt commonArgs;
          decide-audit = craneLib.cargoAudit { inherit src advisory-db; };
          decide-deny = craneLib.cargoDeny commonArgs;
          decide-nextest = craneLib.cargoNextest ({ inherit cargoArtifacts; partitions = 1; partitionType = "count"; } // commonArgs);
        };

        packages =
          let
            clientNpmPackage = pkgs.buildNpmPackage {
              pname = "decide-client";
              version = "0.1.0";
              src = ./client;
              npmDepsHash = "sha256-+la1zQHgD3IGAQRWqVq/l4WTTpiv/SveDvO1z6VE3rs=";
            };
            client = pkgs.runCommand "copy" { } ''
              cp -r ${clientNpmPackage}/lib/node_modules/decide/dist/ $out/
            '';
          in
          {
            decide-llvm-coverage = craneLibLLvmTools.cargoLlvmCov ({ inherit cargoArtifacts; } // commonArgs);
            default = decide;
            client = client;
          } // lib.optionalAttrs (builtins.match "^.*-linux$" system != null) {
            docker = pkgs.dockerTools.buildImage {
              name = "king-decide";
              tag = "latest";
              created = "now";
              runAsRoot = ''
                mkdir -p /app/data/
                chown 1000:1000 /app/data/
                chmod 755 /app/data/
              '';
              config = {
                Env = [ "PATH=${pkgs.dumb-init}/bin/:${decide}/bin" "DECIDE_STATIC_PATH=${client}" ];
                ExposedPorts = { "8000/tcp" = { }; };
                User = "1000:1000";
                Entrypoint = [ "dumb-init" "--" "decide" "0.0.0.0:8000" "sqlite:///app/data/decide.sqlite" ];
              };
            };
          };

        apps.default = flake-utils.lib.mkApp { drv = decide; };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [ sqlx-cli ];
        };
      });
}

