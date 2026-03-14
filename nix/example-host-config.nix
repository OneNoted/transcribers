# Example NixOS host configuration for proteus-tts (vm-111)
# Add to hosts/proteus-tts/default.nix in your NixOS flake
#
# In flake.nix inputs:
#   transcribers.url = "github:OneNoted/transcribers";
#
# In the host config imports:
#   transcribers.nixosModules.default
{ config, pkgs, ... }:

{
  # Enable transcribers — replaces speake-rs + tts-service
  services.transcribers = {
    enable = true;
    listen = "0.0.0.0:9200";

    stt = {
      modelPath = "/var/lib/transcribers/models/ggml-large-v3-turbo.bin";
      language = "auto";
      useGpu = true;
      flashAttn = true;
    };

    tts = {
      model = "custom-voice";
      defaultVoice = "ryan";
    };

    openFirewall = true;
  };

  # NVIDIA GPU support (already configured on proteus-tts)
  hardware.nvidia = {
    open = false;
    package = config.boot.kernelPackages.nvidiaPackages.stable;
  };

  services.xserver.videoDrivers = [ "nvidia" ];

  # Remove the old Docker-based TTS stack:
  # - Delete modules/discord-tts/default.nix
  # - Remove docker-compose stack from ~/services/discord-tts/
  # - Optionally remove Docker entirely if nothing else uses it:
  #   virtualisation.docker.enable = lib.mkForce false;
}
