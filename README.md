<p align="center">
  <img src="assets/acbrinstall.png" width="64" alt="ACBrInstall">
  &nbsp;&nbsp;
  <img src="assets/pascalinstall-256.png" width="64" alt="PascalInstall">
</p>

# ACBrInstall & PascalInstall

Instaladores em Rust para ambientes Pascal e componentes ACBr, com interface gráfica e linha de comando.

[![Build e testes](https://github.com/rftd/ACBrInstall/actions/workflows/build.yml/badge.svg)](https://github.com/rftd/ACBrInstall/actions/workflows/build.yml)
[![Versão 0.3.0](https://img.shields.io/badge/vers%C3%A3o-0.3.0-blue)](https://github.com/rftd/ACBrInstall/releases)
[![Licença MIT](https://img.shields.io/badge/licen%C3%A7a-MIT-green)](LICENSE)

| Aplicativo | Função | Plataformas |
|---|---|---|
| **ACBrInstall** | Baixar, atualizar e instalar componentes ACBr em uma IDE existente. | Lazarus: Linux, Windows e macOS. Delphi: somente Windows. |
| **PascalInstall** | Preparar FPC, Lazarus, compiladores adicionais, Online Package Manager e designer integrado. | Linux, Windows e macOS. |

[Downloads](#downloads) · [Uso](#uso) · [Requisitos](#requisitos) · [CLI](#linha-de-comando) · [Desenvolvimento](#desenvolvimento)

## Downloads

Os dois aplicativos estão disponíveis separadamente nas [Releases](https://github.com/rftd/ACBrInstall/releases):

| Sistema | Arquitetura | Formatos |
|---|---|---|
| Linux | x86-64 | `.AppImage` e executável |
| Windows | x86-64 | `.exe` |
| macOS | Intel e Apple Silicon, em builds separados | `.pkg`, `.app.zip` e executável |

Os arquivos incluem o nome do aplicativo e a plataforma, como `ACBrInstall-linux-x86_64.AppImage`. Builds de desenvolvimento ficam nos artefatos do [GitHub Actions](https://github.com/rftd/ACBrInstall/actions/workflows/build.yml).

No Linux, conceda permissão de execução ao arquivo baixado:

```bash
chmod +x ACBrInstall-linux-x86_64.AppImage
./ACBrInstall-linux-x86_64.AppImage
```

Se FUSE não estiver disponível, acrescente `--appimage-extract-and-run`. Os AppImages usam a glibc de base do Ubuntu 22.04 e as bibliotecas gráficas do sistema.

No macOS, o `.pkg` instala o aplicativo em `/Applications`. Os pacotes usam assinatura ad-hoc e ainda não possuem certificado Developer ID nem notarização Apple.

## Uso

1. **Prepare o ambiente**, se necessário, com o PascalInstall: escolha a pasta, as versões do FPC/Lazarus e os compiladores adicionais. O ambiente inclui OPM, AnchorDockingDsgn e DockedFormEditor.
2. **Abra o ACBrInstall** e selecione a instalação do Lazarus ou Delphi.
3. **Configure os fontes** usando a pasta que contém `Pacotes/Lazarus` ou `Pacotes/Delphi`.
4. **Selecione os componentes**, confira o resumo e inicie a instalação. As dependências são resolvidas automaticamente; o log acompanha a execução.

| Ação | Condição |
|---|---|
| **Apenas instalar** | Fontes locais existentes; não executa SVN. |
| **Baixar e instalar** | Pasta nova ou vazia para checkout. |
| **Atualizar e instalar** | Raiz de uma cópia SVN existente. |

A opção **Baixar fontes para selecionar componentes** permite fazer somente o checkout antes de escolher os pacotes.

No Lazarus, a seleção substitui os componentes ACBr registrados na IDE: os desmarcados são retirados da lista de instalação, preservando os demais pacotes de terceiros. Artefatos antigos da plataforma atual são guardados em `Lib/.acbrinstall-backup` antes da recompilação.

### Dependências de relatórios

- **FortesReport-CE e PowerPDF:** preparados pelo ACBrInstall via OPM quando exigidos pela seleção no Lazarus. Pacotes já registrados são reutilizados. No Delphi, o FortesReport-CE deve estar instalado previamente.
- **FastReport:** integrações disponíveis, desmarcadas por padrão. Exigem uma cópia adquirida, instalada na IDE ou informada em **FastReport (opcional)**. O produto pago não é baixado pelo aplicativo. No Delphi, a pasta informada complementa os caminhos de compilação e não substitui sua instalação na IDE.

Os aplicativos acompanham o tema do sistema e permitem alternar entre claro e escuro. No Linux, a interface registra ícones e atalhos no perfil do usuário, com suporte ao Wayland.

## Requisitos

Downloads exigem internet. Para baixar ou atualizar os fontes ACBr, disponibilize um cliente **SVN** de linha de comando. A instalação requer os compiladores da IDE selecionada.

| Sistema | Preparação do ambiente Pascal |
|---|---|
| Linux | Compilador C, `make`, binutils, `unzip`, SVN e bibliotecas de desenvolvimento GTK2/X11. O PascalInstall verifica as dependências. |
| Windows | Ferramentas preparadas pelo `fpclazup`; SVN no `PATH` para o ACBr, como as command line tools do TortoiseSVN. |
| macOS | Command Line Tools do Xcode (`xcode-select --install`), `make`, `unzip` e `svn` no `PATH`. Lazarus usa Cocoa. |

No Debian/Ubuntu, para compilar o FPC/Lazarus:

```bash
sudo apt install build-essential subversion unzip libgtk2.0-dev libx11-dev \
  libgdk-pixbuf2.0-dev libcairo2-dev libpango1.0-dev libxtst-dev \
  libatk1.0-dev libglib2.0-dev
```

Para Delphi, use uma instalação existente e feche a IDE antes de instalar componentes. O backend aceita compiladores XE2 ou posteriores; a compatibilidade de cada pacote depende dos fontes ACBr.

## Linha de comando

Sem argumentos, os executáveis abrem a interface gráfica. Os exemplos pressupõem comandos disponíveis no `PATH`; no Windows, use a extensão `.exe`.

```bash
# Preparar FPC/Lazarus com compiladores adicionais para Windows
pascalinstall --cli --dir ~/lazarus_dev --fpc stable --laz stable --cross win32,win64

# Configurar OPM e designer em um Lazarus existente
pascalinstall --cli --dir ~/lazarus_dev --configure-ide

# Consultar componentes dos fontes locais
acbrinstall --cli --ide lazarus --acbr-dir ~/ACBr --default-acbr --list-components

# Instalar componentes selecionados a partir dos fontes locais
acbrinstall --cli --ide lazarus --dir ~/lazarus_dev --acbr-dir ~/ACBr \
  --acbr-action install --acbr-packages ACBr_NFe,ACBr_Boleto
```

Para baixar ou atualizar antes de instalar, use `--acbr-action download` ou `--acbr-action update`. Os nomes de pacotes variam conforme a revisão do ACBr; consulte `--list-components`.

| Opção | Função |
|---|---|
| `--cross ""` | Desativar compiladores adicionais no PascalInstall. |
| `--list-environments` | Listar instalações detectadas pelo ACBrInstall. |
| `--default-acbr` | Selecionar os componentes sem FastReport. |
| `--all-acbr` | Selecionar todos, incluindo integrações FastReport. |
| `--fastreport-dir <pasta>` | Informar uma cópia adquirida do FastReport. |

Exemplo para Delphi no PowerShell, ajustando o caminho à instalação local:

```powershell
acbrinstall.exe --cli --ide delphi `
  --delphi-dir 'C:\Program Files (x86)\Embarcadero\Studio\23.0' `
  --acbr-dir C:\ACBr --acbr-action install `
  --acbr-packages ACBr_NFe,DCLACBr_NFe
```

As opções Delphi incluem `--win64`, `--debug`, `--ide64` e `--delphi-key`. Consulte `acbrinstall --help` ou `pascalinstall --help` para a referência completa; `--version` exibe a versão.

A CLI restaura as últimas opções salvas. Em scripts, informe explicitamente ambiente, ação e seleção para obter o comportamento desejado.

Para preparar ambientes de CI ou Docker, ambos aceitam `--build-only` no fluxo Lazarus. O PascalInstall prepara FPC/Lazarus sem adicionar OPM e designer à IDE; o ACBrInstall registra e compila os pacotes e dependências sem alterar os componentes instalados na IDE. Falhas na preparação de cross-compilers interrompem a operação.

## Configurações e logs

Cada aplicativo salva suas opções ao iniciar a operação e gera um log completo por execução. **Salvar log** exporta uma cópia, inclusive durante a instalação.

| Sistema | Configuração | Logs |
|---|---|---|
| Linux | `~/.config/<app>/config.json` | `~/.local/state/<app>/logs` |
| Windows | `%APPDATA%\<App>\config.json` | `%LOCALAPPDATA%\<App>\logs` |
| macOS | `~/Library/Application Support/<App>/config.json` | `~/Library/Logs/<App>` |

Substitua `<app>` por `acbrinstall` ou `pascalinstall`; `<App>`, por `ACBrInstall` ou `PascalInstall`. No Linux, `XDG_CONFIG_HOME` e `XDG_STATE_HOME` são respeitados. No Windows, os logs usam `APPDATA` se `LOCALAPPDATA` não estiver disponível.

## Limitações atuais

A migração do instalador Delphi original é gradual. Cópia de DLLs, opções específicas para C++Builder e edição de defines do `ACBr.inc` ainda não estão disponíveis. A cópia de DLLs será exclusiva do Windows.

A instalação foi validada localmente no Linux. Os builds e testes cobrem as plataformas distribuídas, mas a instalação completa com Delphi e em macOS ainda precisa de validação nesses sistemas.

## Desenvolvimento

Use Rust estável e Cargo. No Windows, o toolchain MSVC exige ferramentas C++ e Windows SDK; no macOS, use os Command Line Tools do Xcode. Para compilar a interface Rust no Debian/Ubuntu:

```bash
sudo apt install build-essential libxkbcommon-dev libwayland-dev libx11-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

```bash
git clone https://github.com/rftd/ACBrInstall.git
cd ACBrInstall
cargo fmt --all --check
cargo test --workspace --locked
cargo build --workspace --release --locked

# Executar a interface durante o desenvolvimento
cargo run -p acbrinstall
cargo run -p pascalinstall
```

Os executáveis ficam em `target/release/`. O workspace contém os aplicativos em `apps/`, os backends e a infraestrutura em `crates/install-core/` e a interface em `crates/install-ui/`.

O [script de empacotamento](scripts/package.py) exige Python 3 e deve ser executado no sistema de destino após o build release:

```bash
python3 scripts/package.py --platform linux-x86_64
# No macOS: --platform macos-x86_64 ou --platform macos-aarch64
```

Os pacotes são gerados em `dist/`. O [workflow](.github/workflows/build.yml) testa e compila os dois aplicativos em cada push e pull request. Ao publicar uma release, compila a tag nas quatro plataformas da matriz e anexa os arquivos após todos os builds passarem.

## Contribuição e licença

Abra uma [issue](https://github.com/rftd/ACBrInstall/issues) com versão, sistema, arquitetura, IDE, passos para reproduzir e log sem dados sensíveis. Para pull requests, descreva a alteração e a validação realizada; execute a formatação e os testes antes do envio.

**Autor:** Rafael Teno DIas · **Licença:** [MIT](LICENSE).

O projeto utiliza os fontes ACBr, o [fpclazup](https://github.com/LongDirtyAnimAlf/Reiniero-fpcup) e o [catálogo OPM](https://packages.lazarus-ide.org/). O ícone do ACBrInstall pertence ao Projeto ACBr; a origem dos recursos está em [assets/README.md](assets/README.md). Componentes e recursos de terceiros mantêm suas respectivas licenças.
