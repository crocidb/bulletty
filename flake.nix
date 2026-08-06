{
  description = "bulletty development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      mkBuildInputs = pkgs: with pkgs; [
        rustc
        cargo
        cargo-dist
        clippy
        rustfmt
        rust-analyzer
        pkg-config
        openssl
        perl
      ];
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = mkBuildInputs pkgs;

            shellHook = ''
              export HOME="$PWD/.dev-home"
              mkdir -p "$HOME"
              echo "Isolated HOME set to: $HOME"
              alias bulletty="cargo run --release --"
            '';
          };
        }
      );

      apps = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = {
            type = "app";
            program = builtins.toString (pkgs.writeShellScript "build-release" ''
              exec ${pkgs.nix}/bin/nix develop path:${self.outPath}#devShells.${system}.default -c cargo build --release "$@"
            '');
          };
          release = self.apps.${system}.default;
        }
      );
    };
}
