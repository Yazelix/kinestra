{
  description = "Kinestra: typed recordings of real terminal applications";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/e9a7635a57597d9754eccebdfc7045e6c8600e6b";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      captureTools = with pkgs; [
        coreutils
        ffmpeg-full
        xorg-server
        xdotool
        picom
        xwallpaper
      ];
      # Compile a consumer's ordinary Rust main against this pinned library.
      mkRecorder =
        {
          name,
          recipe ? ./src/main.rs,
          runtimeInputs ? [ ],
          environment ? { },
        }:
        pkgs.rustPlatform.buildRustPackage {
          pname = name;
          version = "0.2.0";
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          postPatch = "cp ${recipe} src/main.rs";
          cargoBuildFlags = [
            "--bin"
            "kinestra"
          ];
          cargoTestFlags = [ "--lib" ];
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            ${pkgs.lib.optionalString (name != "kinestra") "mv $out/bin/kinestra $out/bin/${name}"}
            wrapProgram $out/bin/${name} --prefix PATH : ${
              pkgs.lib.makeBinPath (captureTools ++ runtimeInputs)
            } ${
              pkgs.lib.concatStringsSep " " (
                pkgs.lib.mapAttrsToList (
                  key: value: "--set ${pkgs.lib.escapeShellArg key} ${pkgs.lib.escapeShellArg (toString value)}"
                ) environment
              )
            }
          '';
          meta = {
            description = "Typed terminal recording orchestration";
            license = pkgs.lib.licenses.asl20;
            mainProgram = name;
            platforms = [ system ];
          };
        };
      kinestra = mkRecorder { name = "kinestra"; };
      captureCheck = mkRecorder {
        name = "kinestra-capture-check";
        recipe = ./tests/capture.rs;
      };
    in
    {
      lib.${system}.mkRecorder = mkRecorder;
      packages.${system}.default = kinestra;
      apps.${system}.default = {
        type = "app";
        program = "${kinestra}/bin/kinestra";
      };
      checks.${system}.capture = pkgs.runCommand "kinestra-capture-check" { } ''
        ${pkgs.coreutils}/bin/timeout --kill-after=10s 60s ${captureCheck}/bin/kinestra-capture-check
        touch "$out"
      '';
    };
}
