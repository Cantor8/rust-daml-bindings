{
  description = "Development environment for rust-daml-bindings";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # Stable toolchain with the components needed for daily work.
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "llvm-tools-preview" ];
        };

        # rustfmt.toml sets `unstable_features = true`, which requires nightly
        # rustfmt. Expose it alongside the stable toolchain so `cargo fmt`
        # workflows can pick it up without replacing the primary rustc.
        rustfmtNightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain:
          toolchain.minimal.override { extensions = [ "rustfmt" ]; });

        cantonVersion = "3.5.1";

        cantonSrc = pkgs.fetchurl {
          url = "https://github.com/digital-asset/canton/releases/download/v${cantonVersion}/canton-open-source-${cantonVersion}.tar.gz";
          sha256 = "0gdjrhiqa2djjam42cdw5m80fcdbq2fcy7bk3jyqv8z9cmmycwby";
        };

        # DPM — the Digital Asset Package Manager — replaces the
        # deprecated Daml Assistant (`daml`). It is a small,
        # statically-linked Go launcher: the actual SDK (including
        # the `dpm build` subcommand that compiles LF2 DARs) is
        # pulled from an OCI registry into $DPM_HOME (default
        # `~/.dpm`) on first use via `dpm install`. So unlike the
        # old self-contained `daml-sdk`, `dpm build` needs network
        # + a writable home the first time it runs.
        #
        # That's an acceptable trade here: the only DAR the repo
        # builds is the checked-in `daml-lf` fixture, and rebuilding
        # it is a rare, manual dev-shell task (see
        # daml-lf/test_resources/README.md) — never part of CI. The
        # SDK version stays pinned in each project's `daml.yaml`
        # (`sdk-version:`), which dpm reads directly, so it isn't
        # duplicated here.
        dpmVersion = "1.0.21";

        dpmSrc = pkgs.fetchurl {
          url = "https://github.com/digital-asset/dpm/releases/download/${dpmVersion}/dpm-${dpmVersion}-linux-amd64.tar.gz";
          hash = "sha256-cQYePs7gKbyIzPwbUTZ+MlcWLzvZHH7cxoivUD/vXlk=";
        };

        # The launcher is a static ELF, so it needs no autoPatchelf.
        # The wrapper puts a JDK (17+) on PATH / JAVA_HOME for the
        # JVM-based SDK components dpm downloads and runs, and points
        # DPM_REGISTRY at the public OCI registry so `dpm install`
        # works out of the box from the bare binary.
        dpm = pkgs.stdenv.mkDerivation {
          pname = "dpm";
          version = dpmVersion;
          src = dpmSrc;
          # Tarball is a flat bundle: ./dpm, ./LICENSE, ./README.md.
          sourceRoot = ".";
          nativeBuildInputs = [ pkgs.makeWrapper ];
          dontConfigure = true;
          dontBuild = true;
          installPhase = ''
            install -Dm755 dpm "$out/libexec/dpm/dpm"
            install -Dm644 LICENSE "$out/share/doc/dpm/LICENSE"
            makeWrapper "$out/libexec/dpm/dpm" "$out/bin/dpm" \
              --set JAVA_HOME ${pkgs.jdk21}/lib/openjdk \
              --set DPM_REGISTRY europe-docker.pkg.dev/da-images/public \
              --prefix PATH : ${pkgs.jdk21}/bin
          '';
          meta = {
            description = "Digital Asset Package Manager — dpm build and the Daml SDK launcher";
            homepage = "https://docs.digitalasset.com/build/3.4/dpm/dpm.html";
            license = pkgs.lib.licenses.asl20;
            platforms = [ "x86_64-linux" ];
          };
        };

        # Canton open-source distribution. Wraps the upstream `bin/canton`
        # launcher so JAVA_HOME points at a managed JDK.
        canton = pkgs.stdenv.mkDerivation {
          pname = "canton";
          version = cantonVersion;
          src = cantonSrc;
          nativeBuildInputs = [ pkgs.makeWrapper ];
          installPhase = ''
            mkdir -p $out/share/canton $out/bin
            cp -r . $out/share/canton/
            makeWrapper $out/share/canton/bin/canton $out/bin/canton \
              --set JAVA_HOME ${pkgs.jdk21}/lib/openjdk \
              --prefix PATH : ${pkgs.jdk21}/bin
          '';
          meta = {
            description = "Canton open-source — Daml synchronizer + participant";
            homepage = "https://www.canton.io/";
            license = pkgs.lib.licenses.asl20;
          };
        };

        # Convenience wrapper that boots the in-memory sandbox defined in
        # rust-daml-bindings/canton/{sandbox.conf,sandbox.canton}.
        canton-sandbox = pkgs.writeShellScriptBin "canton-sandbox" ''
          set -euo pipefail
          # Locate the rust-daml-bindings checkout. Allow override via
          # RUST_DAML_BINDINGS_DIR; otherwise probe upwards from the cwd.
          if [ -n "''${RUST_DAML_BINDINGS_DIR:-}" ]; then
            repo="$RUST_DAML_BINDINGS_DIR"
          else
            d="$PWD"
            while [ "$d" != "/" ]; do
              if [ -f "$d/canton/sandbox.conf" ] && [ -f "$d/canton/sandbox.canton" ]; then
                repo="$d"
                break
              fi
              d=$(dirname "$d")
            done
          fi
          if [ -z "''${repo:-}" ]; then
            echo "canton-sandbox: could not find canton/sandbox.conf — run from inside the rust-daml-bindings checkout, or set RUST_DAML_BINDINGS_DIR." >&2
            exit 1
          fi
          echo "canton-sandbox: using config from $repo/canton" >&2
          exec ${canton}/bin/canton daemon \
            -c "$repo/canton/sandbox.conf" \
            --bootstrap "$repo/canton/sandbox.canton" \
            "$@"
        '';
      in
      {
        packages = {
          inherit canton canton-sandbox dpm;
          default = canton-sandbox;
        };

        devShells = let
          # Rust build surface shared by CI and local dev: the toolchain
          # plus the native deps prost-build / tonic-build and transitive
          # crates need. Everything here resolves from cache.nixos.org, so
          # entering this shell on a cold runner is a fast binary fetch
          # rather than a source build.
          rustPackages = with pkgs; [
            rustToolchain
            rustfmtNightly

            # Build deps for tonic-build / prost-build (daml-grpc, daml-lf).
            protobuf

            # Native deps commonly needed by transitive crates.
            pkg-config
            openssl

            # Mirrors the cargo-deny CI job.
            cargo-deny
          ];

          rustEnv = {
            # Help tonic-build / prost-build find protoc deterministically.
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            PROTOC_INCLUDE = "${pkgs.protobuf}/include";

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };
        in {
          # Lightweight shell for the check / test / clippy / fmt /
          # cargo-deny CI jobs (`nix develop .#ci`). Deliberately EXCLUDES
          # the JDK, Canton and the Daml SDK: those are custom
          # (non-cache.nixos.org) derivations whose ~1 GB closure would be
          # built from scratch on every cold runner. Only the integration
          # job's live sandbox needs them, so keeping them out of this
          # shell keeps the bulk of CI a quick binary-cache fetch.
          ci = pkgs.mkShell {
            packages = rustPackages;
            env = rustEnv;
          };

          # Full local + integration environment: the Rust surface above
          # plus a local Canton sandbox, the Daml SDK, and dev-only cargo
          # tooling. Used by the integration CI job and for daily work.
          default = pkgs.mkShell {
            packages = rustPackages ++ (with pkgs; [
              # Cargo tooling — quality-of-life helpers for local dev.
              cargo-edit
              cargo-outdated
              cargo-audit
              cargo-nextest

              # Local Canton sandbox for integration tests.
              jdk21
              canton
              canton-sandbox

              # DPM for building LF2 .dar fixtures from .daml sources
              # (`dpm build`; downloads the SDK into ~/.dpm on first use).
              dpm

              # Handy for poking the gRPC surface from the shell.
              grpcurl
            ]);

            env = rustEnv // {
              # Used by Canton's launcher.
              JAVA_HOME = "${pkgs.jdk21}/lib/openjdk";
            };

            shellHook = ''
              echo "rust-daml-bindings dev shell"
              echo "  rustc:           $(rustc --version)"
              echo "  cargo:           $(cargo --version)"
              echo "  protoc:          $(protoc --version)"
              echo "  java:            $(java -version 2>&1 | head -1)"
              echo "  canton:          ${canton}/share/canton (v${cantonVersion})"
              echo "  dpm:             ${dpm}/bin/dpm (v${dpmVersion})"
              echo ""
              echo "  Start the sandbox with: canton-sandbox"
              echo "  Ledger API listens on:  localhost:5011"
              echo "  Admin API  listens on:  localhost:5012"
              echo "  Build a DAR with:       dpm build (in a project dir with daml.yaml)"
            '';
          };
        };
      });
}
