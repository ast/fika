{
  description = "fika: an HF group-chat digital mode for amateur radio (SM6WJM)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forAllSystems (
        system: pkgs: {
          default = pkgs.mkShell {
            # libpipewire for the native PipeWire audio backend (pkg-config
            # finds it through buildInputs), plus libclang for its bindgen.
            buildInputs = [ pkgs.pipewire ];
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.llvmPackages.libclang.version}/include -isystem ${pkgs.glibc.dev}/include";
            packages = [
              # Rust toolchain
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.rust-analyzer

              # cpal needs ALSA headers at build time
              pkgs.pkg-config
              pkgs.alsa-lib

              # hamlib ships rigctld, which fika talks to for PTT and frequency
              pkgs.hamlib

              # task runner and audio utilities for test recordings
              pkgs.just
              pkgs.sox

              # lossless PNG optimiser for README screenshots
              pkgs.oxipng

              # pw-cli / pw-record for the live channel and debugging audio
              pkgs.pipewire
            ];

            shellHook = ''
              echo "fika: rustc $(rustc --version | cut -d' ' -f2)  |  $(rigctld --version 2>&1 | head -1)"
            '';
          };
        }
      );
    };
}
