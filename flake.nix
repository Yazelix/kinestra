{
  description = "Kinestra: typed recordings of real terminal applications";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/e9a7635a57597d9754eccebdfc7045e6c8600e6b";
  inputs.wdotool = {
    url = "github:cushycush/wdotool/662d7079b669de164797f46fd437c0cf7854bf82";
    flake = false;
  };

  outputs =
    { self, nixpkgs, wdotool }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      waylandKeys = pkgs.rustPlatform.buildRustPackage {
        pname = "wdotool";
        version = "0.5.3";
        src = wdotool;
        cargoLock.lockFile = "${wdotool}/Cargo.lock";
        postPatch = ''
          substituteInPlace wdotool/Cargo.toml \
            --replace-fail 'wdotool-core = { path = "../wdotool-core", version = "0.5.3" }' \
                           'wdotool-core = { path = "../wdotool-core", version = "0.5.3", default-features = false, features = ["wlr-protocols"] }'
        '';
        cargoBuildFlags = [ "-p" "wdotool" "--no-default-features" ];
        cargoInstallFlags = [ "-p" "wdotool" "--no-default-features" ];
        doCheck = false;
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.libxkbcommon pkgs.wayland ];
        postInstall = ''
          install -Dm444 LICENSE-MIT "$out/share/licenses/wdotool/LICENSE-MIT"
          install -Dm444 LICENSE-APACHE "$out/share/licenses/wdotool/LICENSE-APACHE"
        '';
        meta.license = with pkgs.lib.licenses; [ mit asl20 ];
      };
      captureTools = with pkgs; [
        coreutils
        ffmpeg-full
        xorg-server
        xdotool
        picom
        xwallpaper
        sway-unwrapped
        grim
        wf-recorder
        wtype
        waylandKeys
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
      waylandCheck = mkRecorder {
        name = "kinestra-wayland-check";
        recipe = ./tests/wayland.rs;
        runtimeInputs = [ pkgs.foot pkgs.bash ];
      };
    in
    {
      lib.${system}.mkRecorder = mkRecorder;
      packages.${system}.default = kinestra;
      apps.${system}.default = {
        type = "app";
        program = "${kinestra}/bin/kinestra";
      };
      checks.${system} = {
        capture = pkgs.runCommand "kinestra-capture-check" { } ''
          ${pkgs.coreutils}/bin/timeout --kill-after=10s 60s ${captureCheck}/bin/kinestra-capture-check
          touch "$out"
        '';
        wayland = pkgs.runCommand "kinestra-wayland-check" { } ''
          ${pkgs.coreutils}/bin/timeout --kill-after=10s 60s ${waylandCheck}/bin/kinestra-wayland-check
          touch "$out"
        '';
      };
    };
}
