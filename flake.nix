{
  description = "Unified STT + TTS HTTP server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
          config.cudaSupport = true;
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (p:
          p.rust-bin.stable.latest.default
        );

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;

          nativeBuildInputs = with pkgs; [
            pkg-config
            cmake
          ];

          buildInputs = with pkgs; [
            openssl
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        transcribers = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

      in {
        packages = {
          default = transcribers;
          inherit transcribers;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            rust-analyzer
            cargo-watch
          ];

          inputsFrom = [ transcribers ];
        };
      }
    ) // {
      nixosModules.default = import ./nix/module.nix self;
    };
}
