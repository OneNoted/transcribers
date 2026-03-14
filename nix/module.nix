self:
{ config, lib, pkgs, ... }:

let
  cfg = config.services.transcribers;
  settingsFormat = pkgs.formats.toml { };
  configFile = settingsFormat.generate "transcribers.toml" {
    server.listen = cfg.listen;
    stt = {
      model_path = cfg.stt.modelPath;
      language = cfg.stt.language;
      use_gpu = cfg.stt.useGpu;
      flash_attn = cfg.stt.flashAttn;
    };
    tts = {
      model = cfg.tts.model;
      default_voice = cfg.tts.defaultVoice;
      synthesis_timeout_ms = cfg.tts.synthesisTimeoutMs;
    } // lib.optionalAttrs (cfg.tts.voicesDir != null) {
      voices_dir = cfg.tts.voicesDir;
    };
  };
in
{
  options.services.transcribers = {
    enable = lib.mkEnableOption "transcribers STT+TTS server";

    package = lib.mkPackageOption pkgs "transcribers" {
      default = self.packages.${pkgs.system}.default;
    };

    listen = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0:9100";
      description = "Address and port to listen on.";
    };

    stt = {
      modelPath = lib.mkOption {
        type = lib.types.path;
        default = "/models/ggml-large-v3-turbo.bin";
        description = "Path to the Whisper GGML model file.";
      };

      language = lib.mkOption {
        type = lib.types.str;
        default = "auto";
        description = "Language hint for transcription.";
      };

      useGpu = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable GPU acceleration for Whisper.";
      };

      flashAttn = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable flash attention for Whisper.";
      };
    };

    tts = {
      model = lib.mkOption {
        type = lib.types.str;
        default = "custom-voice";
        description = "TTS model variant (custom-voice or base).";
      };

      voicesDir = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Directory for voice clone profiles.";
      };

      defaultVoice = lib.mkOption {
        type = lib.types.str;
        default = "ryan";
        description = "Default voice for synthesis.";
      };

      synthesisTimeoutMs = lib.mkOption {
        type = lib.types.int;
        default = 90000;
        description = "Maximum synthesis time in milliseconds.";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.transcribers = {
      description = "Transcribers STT+TTS Server";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        ExecStart = "${lib.getExe cfg.package} serve --config ${configFile}";
        Restart = "on-failure";
        RestartSec = 5;

        DynamicUser = true;
        StateDirectory = "transcribers";
        CacheDirectory = "transcribers";

        # GPU access
        DeviceAllow = [
          "/dev/nvidia0 rw"
          "/dev/nvidiactl rw"
          "/dev/nvidia-uvm rw"
          "/dev/nvidia-uvm-tools rw"
        ];
        SupplementaryGroups = [ "video" "render" ];
      };

      environment = {
        RUST_LOG = "info";
        HF_HOME = "/var/cache/transcribers/huggingface";
      };
    };
  };
}
