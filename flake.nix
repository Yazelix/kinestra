{
  description = "Kinestra: repeatable recordings of real terminal applications";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/e9a7635a57597d9754eccebdfc7045e6c8600e6b";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      kinestra = pkgs.writeShellApplication {
        name = "kinestra";
        runtimeInputs = with pkgs; [
          coreutils
          ffmpeg-full
          xorg-server
          xdotool
          picom
          xwallpaper
        ];
        text = ''
          if [[ $# == 0 || $1 == --help ]]; then
            printf 'Usage: kinestra SCENARIO.sh [ARGS...]\nRuns a trusted Bash scenario with isolated X11 capture helpers.\n'
            exit 0
          fi
          scenario=$(realpath "$1")
          shift
          export KINESTRA_ACTIVE=1
          # shellcheck disable=SC1091
          source ${./capture.sh}
          trap kinestra_cleanup EXIT
          trap 'exit 130' INT
          trap 'exit 143' TERM
          # shellcheck disable=SC1090
          source "$scenario" "$@"
        '';
      };
    in
    {
      packages.${system}.default = kinestra;
      apps.${system}.default = {
        type = "app";
        program = "${kinestra}/bin/kinestra";
      };
      checks.${system}.capture =
        pkgs.runCommand "kinestra-capture-check"
          {
            nativeBuildInputs = [
              kinestra
              pkgs.shellcheck
            ];
          }
          ''
            shellcheck ${./capture.sh} ${./tests/capture.sh}
            export CHECK_SCENARIO=${./tests/capture.sh}
            kinestra "$CHECK_SCENARIO"
            touch "$out"
          '';
    };
}
