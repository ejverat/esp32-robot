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

        # Entorno FHS para el firmware: ejecuta los binarios genéricos de Linux del
        # toolchain esp (rustc/clang/gcc) sin necesidad de `nix-ld` en el sistema.
        fhsEnv = pkgs.buildFHSEnv {
          name = "esp32-robot-firmware-fhs";
          targetPkgs = p: with p; [
            # herramientas esp-rs
            rustup
            # rust-analyzer real de nixpkgs: el toolchain 'esp' NO lo incluye y
            # el proxy de rustup cae en recursión infinita buscándolo en /usr/bin.
            # hiPrio para que este binario gane sobre el proxy de rustup en el FHS.
            (hiPrio rust-analyzer)
            espup
            espflash        # incluye `espflash` y `cargo-espflash`
            ldproxy
            cargo-generate
            # herramientas de build de ESP-IDF
            python3
            pkg-config
            cmake
            ninja
            git
            gcc             # host cc + libstdc++/libgcc_s
            libclang
            # librerías que necesitan los binarios extranjeros (clang/gcc/ld de esp)
            zlib
            gmp
            mpfr
            libmpc
            ncurses
            libxml2
            xz
          ];
          runScript = "bash";
          profile = ''
            export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
            if [ -f "$HOME/export-esp.sh" ]; then
              . "$HOME/export-esp.sh"
            else
              echo "⚠️  [firmware-fhs] Toolchain esp no detectado. Ejecuta una única vez: espup install"
              echo "   (luego: exit y reentra con nix develop .#firmware-fhs)"
            fi
            echo "🔧 [firmware-fhs] entorno FHS — target xtensa-esp32-espidf"
            if rustup toolchain list 2>/dev/null | grep -q '^esp'; then
              echo "   ✅ toolchain 'esp' instalado"
            else
              echo "   ❌ toolchain 'esp' NO instalado — verifica: rustup toolchain list"
            fi
            echo "   cd firmware && cargo build"
          '';
        };
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

          # Firmware ESP32: esp-rs sobre ESP-IDF (requiere nix-ld en NixOS)
          firmware = pkgs.mkShell {
            name = "esp32-robot-firmware";
            buildInputs = with pkgs; [
              rustup          # shims cargo/rustc que respetan rust-toolchain.toml
              # el toolchain 'esp' no incluye rust-analyzer; hiPrio para que
              # este gane sobre el proxy de rustup
              (hiPrio rust-analyzer)
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
                echo "⚠️  [firmware] Toolchain esp no detectado. Ejecuta una única vez:"
                echo "   espup install"
                echo "   (luego sal y vuelve a entrar: exit && nix develop .#firmware)"
              fi
              echo "🔧 [firmware] target: xtensa-esp32-espidf"
              if rustup toolchain list 2>/dev/null | grep -q '^esp'; then
                echo "   ✅ toolchain 'esp' instalado"
              else
                echo "   ❌ toolchain 'esp' NO instalado — verifica: rustup toolchain list"
              fi
              echo "   cd firmware && cargo build"
            '';
          };

          # Firmware en entorno FHS: NO requiere nix-ld en el sistema
          firmware-fhs = pkgs.mkShell {
            name = "esp32-robot-firmware-fhs-shell";
            shellHook = ''
              exec ${fhsEnv}/bin/${fhsEnv.name}
            '';
          };

          # Por defecto entra al entorno del servidor
          default = server;
        };
      });
}
