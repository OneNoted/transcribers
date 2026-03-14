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
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems f;

      mkTranscribers = { system, withCuda ? false }:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
            config.allowUnfree = withCuda;
            config.cudaSupport = withCuda;
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain (p:
            p.rust-bin.stable.latest.default
          );

          src = craneLib.cleanCargoSource ./.;

          cudaDeps = with pkgs.cudaPackages; [
            cuda_cudart
            cuda_nvcc
            libcublas
            cuda_cccl
          ];

          commonArgs = {
            inherit src;

            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
              clang
            ] ++ pkgs.lib.optionals withCuda cudaDeps;

            buildInputs = with pkgs; [
              openssl
            ] ++ pkgs.lib.optionals withCuda cudaDeps;

            cargoExtraArgs = if withCuda
              then "--features cuda"
              else "";

            env = pkgs.lib.optionalAttrs withCuda {
              WHISPER_CUBLAS = "1";
              CUDA_COMPUTE_CAP = "89";
              CUDA_ROOT = "${pkgs.cudaPackages.cuda_nvcc}";
              CUDA_PATH = "${pkgs.cudaPackages.cuda_nvcc}";
            };
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        in craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          meta.mainProgram = "transcribers";
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
            overlays = [ rust-overlay.overlays.default ];
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
