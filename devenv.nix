{ pkgs, lib, ... }:

let
  # Decoders for common audio and video formats, and GTK's media playback.
  gstPlugins = with pkgs.gst_all_1; [
    gst-plugins-base
    gst-plugins-good
    gst-plugins-bad
    gst-libav
  ];
in
{
  # https://devenv.sh/basics/
  dotenv.enable = true;

  # https://devenv.sh/packages/
  packages = with pkgs; [
    commitlint-rs
    go-task
    lefthook
    meson
    ninja
    flatpak-builder
    cmake
    glib
    gtk4
    libadwaita
    blueprint-compiler
    desktop-file-utils
    appstream
    cargo-deny
    gitleaks
    pkg-config
    nfpm
    gst_all_1.gstreamer
  ] ++ gstPlugins;

  env.GST_PLUGIN_SYSTEM_PATH_1_0 = lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" gstPlugins;

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  # https://devenv.sh/languages/
  # languages.rust.enable = true;

  # https://devenv.sh/processes/
  # processes.dev.exec = "${lib.getExe pkgs.watchexec} -n -- ls -la";

  # https://devenv.sh/services/
  # services.postgres.enable = true;

  # https://devenv.sh/scripts/

  # https://devenv.sh/basics/
  enterShell = ''
    if [ ! -f .env ]; then
      cp .env.example .env
    fi

    lefthook install
  '';

  # https://devenv.sh/tasks/
  # tasks = {
  #   "myproj:setup".exec = "mytool build";
  #   "devenv:enterShell".after = [ "myproj:setup" ];
  # };

  # https://devenv.sh/tests/
  enterTest = ''
    cargo test --all
  '';

  # https://devenv.sh/git-hooks/
  # git-hooks.hooks.shellcheck.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
