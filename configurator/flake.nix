{
  description = "Flake to develop the faderpunk configurator using nix(OS)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: {
    devShell.x86_64-linux = with nixpkgs.legacyPackages.x86_64-linux;
      mkShell {
        buildInputs = [nodejs pnpm zsh];
        shellHook = "exec zsh";
      };
    devShell.aarch64-darwin = with nixpkgs.legacyPackages.aarch64-darwin;
      mkShell {
        buildInputs = [nodejs pnpm zsh];
        shellHook = "exec zsh";
      };
  };
}
