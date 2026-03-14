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

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.cuda;
      defaultText = lib.literalExpression "transcribers.packages.\${system}.cuda";
      description = "The transcribers package to use. Defaults to CUDA build.";
    };

    listen = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0:9100";
      description = "Address and port to listen on.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "transcribers";
      description = "User to run the service as.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "transcribers";
      description = "Group to run the service as.";
    };

    stt = {
      modelPath = lib.mkOption {
        type = lib.types.str;
        default = "/var/lib/transcribers/models/ggml-large-v3-turbo.bin";
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
        type = lib.types.nullOr lib.types.str;
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

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to open the listen port in the firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = "/var/lib/transcribers";
      createHome = true;
    };

    users.groups.${cfg.group} = { };

    systemd.services.transcribers = {
      description = "Transcribers STT+TTS Server";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        ExecStart = "${lib.getExe cfg.package} serve --config ${configFile}";
        Restart = "on-failure";
        RestartSec = 5;

        StateDirectory = "transcribers";
        CacheDirectory = "transcribers";

        # GPU access — NVIDIA devices
        SupplementaryGroups = [ "video" "render" ];
      };

      environment = {
        RUST_LOG = "info";
        HF_HOME = "/var/lib/transcribers/huggingface";
        XDG_DATA_HOME = "/var/lib/transcribers/data";
      };
    };

    networking.firewall.allowedTCPPorts =
      lib.mkIf cfg.openFirewall [ (lib.toInt (lib.last (lib.splitString ":" cfg.listen))) ];
  };
}
