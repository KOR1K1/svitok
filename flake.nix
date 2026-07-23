{
  # Dev-окружение для сборки Свитка на NixOS без костылей: `nix develop`, затем
  # `cd app && npm install && npm run tauri build`. Даёт нативный .deb/.AppImage
  # под твою систему. Полноценной Nix-деривации пакета тут пока нет - только
  # окружение (см. ROADMAP: reproducible builds).
  description = "Svitok - deterministic paper password manager; dev shell for NixOS";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAll (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              pkg-config
              gobject-introspection
              wrapGAppsHook3
              rustc
              cargo
              nodejs_20
            ];

            buildInputs = with pkgs; [
              # webview и его окружение (Tauri v2 = webkitgtk 4.1 / GTK3)
              webkitgtk_4_1
              gtk3
              libsoup_3
              glib
              glib-networking
              cairo
              pango
              gdk-pixbuf
              atk
              harfbuzz
              librsvg
              openssl
            ];

            shellHook = ''
              echo "Свиток dev shell. Собрать: cd app && npm install && npm run tauri build"
              # частый фикс чёрного/пустого webview на части GPU и под Wayland
              export WEBKIT_DISABLE_DMABUF_RENDERER=1
            '';
          };
        });
    };
}
