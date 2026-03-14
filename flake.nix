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
    let
      # Systems that make sense for this project
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems f;

      # Build package for a given system with optional CUDA
      mkTranscribers = { system, withCuda ? false }:
        let
          pkgs = import nixpkgs {
            inherit system;
            config.allowUnfree = withCuda;
            config.cudaSupport = withCuda;
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain (p:
            p.rust-bin.stable.latest.default
          );

          src = craneLib.cleanCargoSource ./.;

          # whisper-rs-sys needs these to find/build whisper.cpp
          whisperBuildInputs = with pkgs; [ cmake clang ]
            ++ pkgs.lib.optionals withCuda (with pkgs.cudaPackages; [
              cuda_cudart
              cuda_nvcc
              libcublas
              cuda_cccl
            ]);

          # candle CUDA needs cuBLAS at link time
          candleBuildInputs = pkgs.lib.optionals withCuda (with pkgs.cudaPackages; [
            libcublas
            cuda_cudart
          ]);

          commonArgs = {
            inherit src;

            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              clang
            ] ++ pkgs.lib.optionals withCuda [
              pkgs.cudaPackages.cuda_nvcc
            ];

            buildInputs = with pkgs; [
              openssl
            ] ++ whisperBuildInputs ++ candleBuildInputs;

            # Feature flags
            cargoExtraArgs = if withCuda
              then "--features cuda"
              else "";

            # Environment for whisper-rs-sys CUDA build
            env = pkgs.lib.optionalAttrs withCuda {
              WHISPER_CUBLAS = "1";
              CUDA_COMPUTE_CAP = "89";
            };
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        in craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
    in
    {
      packages = forAllSystems (system: {
        default = mkTranscribers { inherit system; };
        cpu = mkTranscribers { inherit system; };
        cuda = mkTranscribers { inherit system; withCuda = true; };
      });

      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            config.allowUnfree = true;
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain (p:
            p.rust-bin.stable.latest.default
          );
        in {
          default = craneLib.devShell {
            packages = with pkgs; [
              rust-analyzer
              cargo-watch
              cmake
              clang
              pkg-config
              openssl
            ];
          };
        }
      );

      nixosModules.default = import ./nix/module.nix self;
    };
}
