{
  description = "Build a cargo project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/release-23.05";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        inherit (pkgs) lib;

        craneLib = crane.lib.${system};
        src = lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type: (craneLib.filterCargoSources path type) || (builtins.match ".*/migrations/.*$" path != null);
        };


        # Common arguments can be set here to avoid repeating them later
        commonArgs = {
          inherit src;
          pname = "decide";
          version = "0.1.0";
          strictDeps = true;

          buildInputs = [
            # Add additional build inputs here
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            pkgs.libiconv
          ];

          # Additional environment variables can be set directly
          # MY_CUSTOM_VAR = "some value";
        };

        craneLibLLvmTools = craneLib.overrideToolchain
          (fenix.packages.${system}.complete.withComponents [
            "cargo"
            "llvm-tools"
            "rustc"
          ]);

        # Build *just* the cargo dependencies, so we can reuse
        # all of that work (e.g. via cachix) when running in CI
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the actual crate itself, reusing the dependency
        # artifacts from above.
        decide = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      {
        checks = {
          # Build the crate as part of `nix flake check` for convenience
          inherit decide;

          # Run clippy (and deny all warnings) on the crate source,
          # again, resuing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          decide-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          decide-doc = craneLib.cargoDoc (commonArgs // {
            inherit cargoArtifacts;
          });

          # Check formatting
          decide-fmt = craneLib.cargoFmt commonArgs;

          # Audit dependencies
          decide-audit = craneLib.cargoAudit {
            inherit src advisory-db;
          };

          # Audit licenses
          decide-deny = craneLib.cargoDeny commonArgs;

          # Run tests with cargo-nextest
          # Consider setting `doCheck = false` on `decide` if you do not want
          # the tests to run twice
          decide-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
          });
        };

        packages =
          let
            client = pkgs.buildNpmPackage {
              pname = "decide-client";
              version = "0.1.0";
              src = ./client;
              npmDepsHash = "sha256-HEAywFrsNZ8kBGEa58txtdujrLmdjFtzo2JISK4iCag=";
            };
          in
          {
            decide-llvm-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs // {
              inherit cargoArtifacts;
            });
            default = decide;
            client = client;
            docker = pkgs.dockerTools.buildImage {
              name = "decide";
              tag = "latest";
              created = "now";
              runAsRoot = ''
                #!${pkgs.runtimeShell}
                mkdir -p /app/data/
                chown 1000:1000 /app/data/
                chmod 755 /app/data/
              '';
              config = {
                Env = [
                  "PATH=${pkgs.dumb-init}/bin/:${decide}/bin"
                  "DECIDE_STATIC_PATH=${client}/lib/node_modules/decide/dist"
                ];
                ExposedPorts = {
                  "8000/tcp" = { };
                };
                User = "1000:1000";
                Entrypoint = [ "dumb-init" "--" "decide" "0.0.0.0:8000" "sqlite:///app/data/decide.sqlite" ];
              };
            };
          };

        apps.default = flake-utils.lib.mkApp {
          drv = decide;
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

          # Extra inputs can be added here; cargo and rustc are provided by default.
          packages = [
            # pkgs.ripgrep
          ];
        };
      });
}
