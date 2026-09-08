{
  description = "ESP32 Robot — monorepo: dev environments en Rust (server + firmware)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells = rec {
          # Backend central: Axum + Tokio
          server = pkgs.mkShell {
            name = "esp32-robot-server";
            buildInputs = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              cargo-watch
              pkg-config
              openssl
              git
            ];
            shellHook = ''
              echo "🦀 [server] Rust $(rustc --version)"
              echo "   cd server && cargo run"
            '';
          };

          # Firmware ESP32: esp-rs sobre ESP-IDF
          firmware = pkgs.mkShell {
            name = "esp32-robot-firmware";
            buildInputs = with pkgs; [
              rustup          # shims cargo/rustc que respetan rust-toolchain.toml
              espup           # instala el toolchain Rust de Espressif (Xtensa/RISC-V)
              espflash        # incluye `espflash` y `cargo-espflash`
              ldproxy         # linker proxy para los targets espidf
              cargo-generate  # para generar proyectos desde plantillas
              python3
              pkg-config
              cmake
              ninja
              git
              libclang
            ];
            shellHook = ''
              export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
              if [ -f "$HOME/export-esp.sh" ]; then
                . "$HOME/export-esp.sh"
              else
                echo "⚠️  [firmware] Toolchain esp no detectado. Ejecuta una única vez: espup install"
                echo "   (vuelve a entrar al shell para cargar \$HOME/export-esp.sh)"
              fi
              echo "🔧 [firmware] target: xtensa-esp32-espidf"
              echo "   cd firmware && cargo build"
            '';
          };

          # Por defecto entra al entorno del servidor
          default = server;
        };
      });
}
