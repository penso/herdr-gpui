{
  description = "Sheprd: a native GPUI client for an existing local Herdr daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # Build with exactly the toolchain rust-toolchain.toml pins, not whatever
    # rustc the nixpkgs revision happens to carry.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;
      # GPUI's Linux backend is the only one this flake builds; macOS users
      # install the signed DMG or the Homebrew cask instead.
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
      package =
        pkgs:
        let
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          # Loaded with dlopen at runtime, so the linker never records them.
          runtimeLibraries = [
            pkgs.vulkan-loader
            pkgs.wayland
          ];
        in
        rustPlatform.buildRustPackage {
          pname = "sheprd";
          version = "0-unstable-${lib.substring 0 8 (self.lastModifiedDate or "19700101")}";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./rust-toolchain.toml
              ./crates
              ./assets
              # The agent skill is compiled into the executable.
              ./skills
              ./scripts/release/so.pen.herdr-gpui.desktop
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          # The app starts chats with the sheprd-acp beside its executable,
          # runs the sheprd-remote-control beside it as a user service, runs
          # automations in the sheprd-automate there, and starts sheprd-nearby
          # for nearby pairing and presence over Bluetooth.
          cargoBuildFlags = [
            "-p"
            "sheprd-gpui"
            "-p"
            "sheprd-acp"
            "-p"
            "sheprd-remote-control"
            "-p"
            "sheprd-automate"
            "-p"
            "sheprd-nearby"
          ];
          # The optimized CLI suite CI's main-branch build runs; the rest of
          # the workspace suite needs a desktop or a live daemon.
          cargoTestFlags = [
            "-p"
            "sheprd-gpui"
            "--test"
            "cli"
          ];
          nativeBuildInputs = [
            pkgs.makeWrapper
            pkgs.pkg-config
            rustPlatform.bindgenHook
          ];
          buildInputs = [
            pkgs.alsa-lib
            pkgs.fontconfig
            pkgs.freetype
            pkgs.libxkbcommon
            pkgs.libxcb
          ]
          ++ runtimeLibraries;
          # No updater key is embedded, so this build never replaces itself;
          # Nix owns the store path and upgrades come from the flake.
          postInstall = ''
            install -Dm644 scripts/release/so.pen.herdr-gpui.desktop \
              $out/share/applications/so.pen.herdr-gpui.desktop
            install -Dm644 assets/icons/sheprd-linux.svg \
              $out/share/icons/hicolor/scalable/apps/sheprd.svg
          '';
          postFixup = ''
            patchelf --add-rpath ${lib.makeLibraryPath runtimeLibraries} $out/bin/sheprd
            wrapProgram $out/bin/sheprd \
              --prefix PATH : ${lib.makeBinPath [ pkgs.wl-clipboard pkgs.xclip ]}
            # Point at the final wrapper, including its clipboard PATH setup.
            ln -s sheprd $out/bin/herdr-gpui
            for helper in acp automate nearby remote-control; do
              ln -s "sheprd-$helper" "$out/bin/herdr-$helper"
            done
          '';
          meta = {
            description = "Native GPUI client for an existing local Herdr daemon";
            homepage = "https://sheprd.org";
            license = lib.licenses.asl20;
            mainProgram = "sheprd";
            platforms = systems;
          };
        };
    in
    {
      packages = forAllSystems (pkgs: {
        default = package pkgs;
        sheprd = package pkgs;
        herdr-gpui = package pkgs;
      });
      overlays.default = final: _prev: {
        sheprd = self.packages.${final.stdenv.hostPlatform.system}.default;
        herdr-gpui = self.packages.${final.stdenv.hostPlatform.system}.default;
      };
    };
}
