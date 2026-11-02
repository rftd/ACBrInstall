# PascalInstall e ACBrInstall

Dois programas independentes no mesmo workspace Rust:

| Programa | Responsabilidade | Plataformas |
|---|---|---|
| **PascalInstall** | Preparar FPC, Lazarus, cross-compilers, Online Package Manager e designer integrado | Linux, Windows e macOS (Intel/Apple Silicon) |
| **ACBrInstall** | Baixar/atualizar fontes e instalar componentes ACBr em uma IDE existente | Lazarus: Linux/Windows/macOS. Delphi: somente Windows |

O ACBrInstall não instala ou atualiza FPC/Lazarus. O PascalInstall não baixa,
compila ou registra componentes ACBr. Downloads, processos, cancelamento, logs e
persistência são compartilhados pela biblioteca `install-core`; tema, seletores
de pasta e acompanhamento são compartilhados por `install-ui`.

```text
apps/
  acbrinstall/       executável ACBrInstall
  pascalinstall/     executável PascalInstall
crates/
  install-core/      infraestrutura e backends Pascal, ACBr/Lazarus e ACBr/Delphi
  install-ui/        interface comum, com telas específicas de cada programa
```

## Compilar e executar

```bash
cargo test --workspace
cargo build --workspace --release
cargo run -p pascalinstall
cargo run -p acbrinstall
```

Os executáveis são `target/release/pascalinstall` e
`target/release/acbrinstall` (com `.exe` no Windows). Sem argumentos, cada um abre
sua própria interface. Argumentos usam a CLI, que também aceita `--cli`.

As interfaces abrem seguindo o tema do sistema. No Linux, consultam a preferência
do [portal de aparência](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)
via `dbus-send` ou `gdbus`, com alternativas pelas configurações KDE/GNOME e
`GTK_THEME` quando necessário. Alterações são verificadas em segundo plano.
O botão de sol/lua permite uma escolha manual; o clique direito oferece
**Seguir tema do sistema** para restaurar o acompanhamento automático.

## Preparar FPC e Lazarus

```bash
pascalinstall --cli --dir ~/lazarus_dev --fpc stable --laz stable --cross win32,win64
# Somente configurar OPM e designer em um Lazarus existente
pascalinstall --cli --dir ~/lazarus_dev --configure-ide
```

O PascalInstall usa o CLI **fpclazup**, do repositório
[Reiniero-fpcup](https://github.com/LongDirtyAnimAlf/Reiniero-fpcup).
A consulta à API seleciona o binário conforme sistema e arquitetura. FPC e
Lazarus podem usar `stable` ou tags oficiais de release, carregadas em segundo
plano. No macOS, o bootstrap nativo usa `darwin` e o widgetset **Cocoa**.
A primeira compilação pode demorar bastante.

O ambiente preparado inclui **Online Package Manager**, **AnchorDockingDsgn**
e **DockedFormEditor**. O reparo `--configure-ide` preserva os componentes
anteriormente instalados na IDE.

### Dependências

Linux: compilador C, make, binutils, SVN, unzip e bibliotecas de desenvolvimento
GTK2/X11. O programa verifica os requisitos e informa o comando da distribuição.

```bash
# Debian/Ubuntu
sudo apt install build-essential subversion unzip libgtk2.0-dev libx11-dev libgdk-pixbuf2.0-dev libcairo2-dev libpango1.0-dev libxtst-dev libatk1.0-dev libglib2.0-dev
```

Windows: o fpclazup pode preparar as ferramentas necessárias; para baixar ACBr,
use um cliente SVN de linha de comando, como o incluído opcionalmente no
TortoiseSVN.

macOS: **Command Line Tools do Xcode** (`xcode-select --install`), `make`, `unzip`
e cliente `svn` no PATH. Não são exigidas bibliotecas GTK2. O
[Lazarus oficial](https://www.lazarus-ide.org/index.php?page=downloads) oferece
versões para Intel e Apple Silicon.

## Instalar ACBr no Lazarus

A interface do ACBrInstall segue a organização do assistente original: **Bem-vindo**,
**Ambiente**, **Configurações**, **Componentes**, **Instalação** e **Conclusão**.
As etapas ficam na lateral, com navegação **Voltar/Próximo** no rodapé. A lista
agrupa os pacotes por família, com busca, filtro e descrições. A etapa de instalação
mostra um resumo antes de iniciar o build; depois apresenta o log e a conclusão.
O visual usa a paleta compartilhada dos aplicativos e acompanha o tema do sistema.

Selecione uma instalação existente detectada ou informe a pasta do ambiente
criado pelo PascalInstall. Também são aceitas pastas que contêm `lazbuild`
diretamente, como instalações oficiais de Lazarus.

```bash
acbrinstall --cli --dir ~/lazarus_dev --acbr-dir ~/ACBr --acbr-action install
acbrinstall --cli --dir ~/lazarus_dev --acbr-dir ~/ACBr --acbr-action download
acbrinstall --cli --dir ~/lazarus_dev --acbr-dir ~/ACBr --acbr-action update
acbrinstall --cli --list-environments
```

**Apenas instalar** usa os fontes locais sem SVN. **Baixar e instalar** exige
uma pasta nova/vazia; **Atualizar e instalar** exige uma cópia SVN existente.
A pasta dos fontes contém `Pacotes/Lazarus` e/ou `Pacotes/Delphi`.

A lista de componentes tem busca, famílias, checkboxes, **Marcar todos** e
**Desmarcar todos**. Dependências são incluídas automaticamente e compiladas na
ordem necessária. Em uma pasta nova, **Baixar fontes para selecionar componentes**
faz somente o checkout e volta à lista antes de compilar.

```bash
acbrinstall --cli --acbr-dir ~/ACBr --acbr-action install --acbr-packages ACBr_NFe,ACBr_Boleto
acbrinstall --cli --acbr-dir ~/ACBr --all-acbr --list-components
acbrinstall --cli --acbr-dir ~/ACBr --default-acbr --acbr-action install
# Para integrações FastReport, use sua instalação existente ou informe os fontes adquiridos
acbrinstall --cli --acbr-dir ~/ACBr --acbr-packages ACBr_NFSeXDANFSeFR --fastreport-dir ~/FastReport
```

**FastReport permanece disponível**, desmarcado na seleção padrão. **Marcar todos**
e `--all-acbr` incluem essas integrações; **Seleção padrão** e `--default-acbr`
restauram os componentes que não exigem FastReport. Para usá-lo, instale sua cópia
adquirida na IDE ou informe a pasta em **FastReport (opcional)** / `--fastreport-dir`.
No Lazarus, essa pasta é pesquisada por `.lpk` e os pacotes são registrados e
compilados antes do ACBr. O instalador não baixa o produto pago. Se uma integração
selecionada exigir um pacote ausente, o build informa qual dependência instalar.

O **FortesReport-CE** (`frce`) e o **PowerPDF** (`pack_powerpdf`) são preparados
pelo catálogo oficial do [OPM](https://packages.lazarus-ide.org/) quando exigidos
pelos componentes Lazarus selecionados. Pacotes já registrados são reutilizados;
os downloads ficam em `<ambiente>/packages/opm`.
Essa preparação pertence ao **ACBrInstall**. FortesReport-CE e PowerPDF não são
instalados pelo PascalInstall ao preparar o Lazarus.

Artefatos antigos da plataforma atual (`.ppu`, `.o`, `.a`, `.compiled`) são
movidos para `Lib/.acbrinstall-backup` antes da recompilação. No Linux/macOS,
caminhos de `.lpk` com capitalização incorreta são corrigidos quando existe uma
correspondência inequívoca, com backup do original. Os fontes Pascal e os
artefatos de outras plataformas são preservados. Os componentes ACBr desmarcados
são retirados da lista de instalação da IDE, preservando os demais pacotes.

## Instalar ACBr no Delphi — Windows

O backend Delphi inicial lê os `.dpk` de `Pacotes/Delphi`, resolve dependências,
compila os pacotes com os compiladores da instalação selecionada e registra os
BPLs de design na IDE. A referência de comportamento é o instalador original
em `Projetos/ACBrInstall Trunk2` dos fontes ACBr.

- Detecta perfis Delphi/RAD Studio no registro do usuário (Embarcadero/CodeGear/Borland).
- Suporta compiladores Delphi XE2 ou posteriores; cada componente ainda depende
  da compatibilidade de seus fontes com a versão escolhida.
- Compila Release/Win32; permite também Debug e bibliotecas Win64.
- Instala na IDE de 32 bits e, opcionalmente, na IDE de 64 bits quando disponível.
- Mantém DCU/DCP/BPL separados por versão, plataforma e modo.
- Faz backup `.reg` antes de alterar Known Packages e caminhos da IDE.
- Preserva os pacotes de terceiros e permite selecionar integrações FastReport.

```powershell
acbrinstall.exe --cli --ide delphi --list-environments
acbrinstall.exe --cli --ide delphi --delphi-dir 'C:\Program Files (x86)\Embarcadero\Studio\23.0' --acbr-dir C:\ACBr --acbr-action install --acbr-packages ACBr_NFe,DCLACBr_NFe
```

A versão detectada fornece a chave do registro; também pode ser informada por
`--delphi-key 'HKCU\Software\Embarcadero\BDS\23.0'`. Use `--win64`, `--debug` e
`--ide64` conforme necessário. Feche o Delphi durante a instalação.
Relatórios Fortes exigem **FortesReport-CE já instalado no Delphi**; o catálogo
OPM é utilizado apenas pelo backend Lazarus. FastReport exige uma cópia adquirida
compatível com a versão e plataforma do Delphi, com seus DCP/BPL nos caminhos da
IDE. A pasta opcional `--fastreport-dir` acrescenta caminhos de compilação;
subpastas `Win32/Release`, `Win64/Release` e os equivalentes `Debug` têm prioridade
sobre a raiz. Ela não substitui a instalação do FastReport no Delphi.

A migração do instalador original é gradual: cópia de DLLs, opções específicas
para C++Builder, edição de defines do `ACBr.inc` e outros ajustes avançados do
instalador Delphi original ainda não estão disponíveis nesta interface.
A cópia de DLLs, quando migrada, será exclusiva do Windows, inclusive para
Lazarus nesse sistema. Linux e macOS não terão essa opção nem executarão essa etapa.

## Configurações

Cada programa salva as últimas opções ao iniciar seu build, inclusive se falhar:

| Sistema | ACBrInstall | PascalInstall |
|---|---|---|
| Linux | `~/.config/acbrinstall/config.json` | `~/.config/pascalinstall/config.json` |
| Windows | `%APPDATA%\ACBrInstall\config.json` | `%APPDATA%\PascalInstall\config.json` |
| macOS | `~/Library/Application Support/ACBrInstall/config.json` | `~/Library/Application Support/PascalInstall/config.json` |

No Linux, `XDG_CONFIG_HOME` substitui `~/.config`. Argumentos explícitos da CLI
sobrescrevem os valores restaurados. O arquivo antigo do instalador combinado
continua compatível: os campos de FPC/Lazarus são migrados para o PascalInstall
antes de o ACBrInstall salvar seu formato independente. A seleção ACBr usa
`acbr_packages`: `null` usa a seleção padrão; uma lista contém os nomes escolhidos.
`all_components: true` com seleção `null` inclui também FastReport.
`fastreport_dir` guarda a pasta opcional do produto adquirido.
`ide` distingue `lazarus` e `delphi`.

## Ícones

O ACBrInstall usa o ícone do instalador Delphi original. O PascalInstall usa um
ícone próprio, com a letra P e uma seta de instalação. Ambos são incorporados à
janela, ao executável Windows e aos pacotes de distribuição. Os arquivos, a
origem e o prompt do ícone gerado estão em [assets/README.md](assets/README.md).

No Linux, a primeira abertura da interface registra o arquivo `.desktop` e o
ícone em `~/.local/share/applications` e `~/.local/share/icons` (ou em
`XDG_DATA_HOME`). O identificador Wayland corresponde ao nome desse arquivo:
`dev.rftd.acbrinstall` / `dev.rftd.pascalinstall`. Isso permite ao ambiente gráfico
encontrar os ícones da barra de tarefas e da decoração da janela. Ao executar um
AppImage, o atalho aponta para o arquivo original, e não para a montagem temporária.
Os comandos de CLI não registram atalhos.

## Validação e releases

O GitHub Actions testa o workspace e gera **os dois programas** para Linux
x86-64, Windows x86-64, macOS Intel e macOS Apple Silicon:

- Linux: executáveis e **AppImage** por programa.
- Windows: executáveis `.exe` com seus ícones incorporados.
- macOS: aplicativos `.app` em ZIP e instaladores `.pkg` para `/Applications`,
  em builds separados para Intel e Apple Silicon.

O workflow executa builds e testes em **todo push**, em qualquer branch ou tag,
e nos **pull requests** ao abrir, reabrir ou atualizar os commits. Os instaladores
ficam disponíveis como artefatos da execução no GitHub Actions.

Ao **publicar uma release**, o workflow compila e testa a tag correspondente nas
quatro plataformas. Depois que todos os builds passam, baixa os artefatos dessa
execução e os anexa à release publicada. Isso também vale para pré-releases;
criar apenas uma tag não publica uma release automaticamente.

Os runners macOS usados seguem o catálogo de
[imagens oficiais do GitHub](https://github.com/actions/runner-images).

O script [scripts/package.py](scripts/package.py) empacota os dois aplicativos,
inclui os ícones e testa a CLI de cada pacote. No Linux, usa releases fixos do
[appimagetool](https://github.com/AppImage/appimagetool) e runtime, com SHA256
verificado; não exige FUSE durante o build. Os AppImages usam a glibc de base do
Ubuntu 22.04 e as bibliotecas gráficas do sistema. No macOS, usa `codesign` ad-hoc,
`ditto` e `pkgbuild`; os pacotes ainda não têm certificado Developer ID nem
notarização Apple.

```bash
cargo build --workspace --release --locked
python3 scripts/package.py --platform linux-x86_64
# No macOS, escolha macos-x86_64 ou macos-aarch64 conforme o build nativo
python3 scripts/package.py --platform macos-aarch64
# Inspecionar apenas AppDir/.app, sem ferramentas nativas de empacotamento
python3 scripts/package.py --platform macos-aarch64 --bundle-only
```

A validação local é feita no Linux. A instalação real com Delphi e a execução em
macOS exigem validação nesses sistemas; compilar/testar em CI não substitui esse
teste de instalação com uma IDE real.
