# Firefox Codex MCP

제품의 초점은 Codex 채팅별 탭 격리·이어쓰기·철회와 쉬운 설치입니다.
관련 도구와 [동일 조건 비교 계획](docs/comparison-plan.md)을 공개하며,
아직 직접 측정하지 않은 성능·보안 우위를 주장하지 않습니다.

[English](README.md) · **한국어**

![Codex Work Tabs 커서](extension/icons/cursor-96.png)

Codex가 사용 중인 Firefox의 로그인 상태로 **자신의 작업 탭을 직접 열고 사용하는** 로컬 MCP입니다.
기본 연결은 Firefox 확장 + 로컬 연결 프로그램입니다. 코드·토큰·프로필 경로를 입력하거나
탭마다 허용할 필요가 없습니다. 기존 WebDriver BiDi 방식은 고급 옵션으로 유지합니다.

**Linux 0.4.3 정식판 · Windows 0.5.0-beta.3 프리뷰.** 처음 설치한 뒤에는 일반 Firefox의
**Codex Work Tabs → Settings**에서 관리합니다. 별도 Firefox 앱 메뉴 항목이나 전용 프로필을
만들지 않습니다. RPM·DEB는 시스템에 한 번 설치하고 OS 계정마다 Settings에서 Codex를
연결합니다. 확장만으로 연결 프로그램을 설치할 수는 없습니다. [관리 안내](docs/firefox-only-management.md)를 참고하세요. 과거 배포 파일은
변경하지 않고 그대로 남깁니다.

```text
Codex 채팅 A ─ MCP stdio ─┐                       ┌─ A의 새 작업 탭
                         ├─ Rust 로컬 라우터 ─ 확장 ┤
Codex 채팅 B ─ MCP stdio ─┘   private Unix socket   └─ B의 새 작업 탭
                              Native Messaging
```

## 설치와 사용

Linux x86_64의 일반 Firefox 140+와 로컬 Codex 앱/CLI가 필요합니다.
패키지는 Rust·Node 설치 없이 실행됩니다. Ubuntu 22.04(glibc 2.35) 이상 대상이며
Windows는 아래 별도 프리뷰를 제공합니다. macOS, Snap/Flatpak, Android는 검증하지 않았습니다.

**0.4.3은 영문 Linux 정식판입니다.** RPM·DEB가 기본이며 관리자 권한이 없으면 tar.gz를
사용합니다. 설치 화면·확장·진단은 영어입니다. [릴리스·체크섬·검증 기록](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.3)을 제공합니다.
Windows beta.3 배포 파일은 그대로 유지합니다.

1. 일반 Firefox를 준비하고 로컬 Codex 앱/CLI에 로그인합니다.
2. 배포판에 맞는 파일을 소프트웨어 관리자로 설치합니다.

   | 배포판 | 다운로드 | 다운로드 폴더에서 실행할 명령 |
   | --- | --- | --- |
   | Fedora | [RPM x86_64](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp-0.4.3-1.x86_64.rpm) | `sudo dnf install ./firefox-codex-mcp-0.4.3-1.x86_64.rpm` |
   | Ubuntu / Debian | [DEB amd64](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp_0.4.3-1_amd64.deb) | `sudo apt install ./firefox-codex-mcp_0.4.3-1_amd64.deb` |

3. Firefox에서 [Mozilla 서명 Codex Work Tabs XPI](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/codex-work-tabs-0.4.2.xpi)를 열고 확장 설치를 확인합니다. 파일로 저장되면 부가 기능 관리자 → 톱니바퀴 → **파일에서 부가 기능 설치**로 선택합니다.
4. 일반 사용자로 **Codex Work Tabs → Settings → Connect Codex → Continue**를 누릅니다. 계정의 MCP·스킬·사용 지침을 자동 등록하므로 따로 설정하지 않습니다. 이 단계는 `sudo`로 실행하지 않습니다.
5. 새 Codex 채팅에 `https://example.com을 브라우저로 열고, Learn more 링크를 눌러 이동한 페이지 제목을 알려줘.`라고 요청합니다.

확장이 이미 설치됐다면 RPM/DEB 설치 후 Settings를 열면 됩니다. 일반 사용자로
`firefox-codex-mcp --manage`를 실행해 관리 화면과 확장 설치 링크를 열 수도 있습니다.
관리자 권한이 없다면 [tar.gz 보조 배포](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp-0.4.3-linux-x86_64.tar.gz)를
완전히 풀고 `./install.sh` → **Install connection**을 사용합니다. RPM/DEB가 이미 있다면
개인 복사본 대신 시스템 프로그램에 연결합니다. [처음 설치 안내](docs/first-install-linux.ko.md)와
[패키지 소유권·이전·복구](docs/linux-packages.md)를 참고하세요.

새 작업 탭과 링크 클릭이 확인되면 실제 연결이 된 것입니다. 도구 승인은 자신이 요청한
작업인지 확인한 뒤 허용합니다. 자동 검증은 초보자의 직접 사용성 시험과 구분합니다.
Linux 0.4.3은 Mozilla가 서명한 영문 확장 `codex-work-tabs-0.4.2.xpi`를 포함합니다.
아래는 기존 **0.4.0 연결 프로그램 + 확장**으로 확인한 검증 기록입니다.
일반 Firefox의 영구 설치·재시작·다중 프로필을 검증했습니다.
현재 개인 프로필의 실제 Codex 데스크톱 두 채팅에서 여러 턴에 걸쳐 동일 탭·문서·입력값이
유지되고, Count를 이어서 클릭하며 다른 채팅의 탭 선택은 거부되는 것을 확인했습니다.
철회한 탭은 다음 턴에도 복구되지 않습니다.
[0.4.0 데스크톱·Firefox 검증 기록](docs/chat-continuity-e2e-2026-11-17.json).
AMO 검색 목록에는 게시하지 않은 직접 배포판입니다. `*-unsigned.xpi`는 개발·심사 제출용이며
일반 설치에 사용하지 마세요. [서명 정보](packaging/SIGNING.md)와
[검증 범위](docs/extension-default-e2e-2026-11-17.json)를 확인하세요.

확장만으로 컴퓨터에 연결 프로그램을 설치할 수는 없으므로 최초 두 구성요소 설치가 필요합니다.
그 이후에는 일반 Firefox 실행기를 그대로 사용합니다. 프로필이 하나면 자동 선택하며,
여러 프로필이 처음 연결되면 원하는 Firefox 확장에서 **Use this Firefox**를 한 번 누릅니다.
선택한 프로필이 꺼져 있으면 다른 프로필로 자동 전환하지 않습니다.
Firefox가 모두 꺼졌을 때 일반 Firefox 실행을 시도합니다. Firefox 자체의 기본 프로필이
선택한 프로필과 다르면 해당 프로필을 직접 열어야 합니다. 경로를 추측하지 않습니다.

전역 MCP·스킬·짧은 사용 지침을 설치해 브라우저가 필요한 작업에서 AI가 Firefox를 고려하도록
합니다. 모든 모델의 도구 선택을 강제하는 기능은 아닙니다. 이미 열린 채팅은 새 채팅/앱 재시작이
필요할 수 있고 프로젝트 설정·관리자 정책이 우선합니다. 클라우드 채팅에는 적용되지 않습니다.
Codex의 **컴퓨터 사용 목록**을 패치하지 않고 MCP로 연결합니다.

## Windows 프리뷰

[v0.5.0-beta.3](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.5.0-beta.3)은
Windows 10 22H2·Windows 11 x64와 일반 데스크톱 Firefox를 대상으로 합니다.
자동 검증 환경은 Windows Server 2022이며, **Windows 10·11 데스크톱에서의 실제 검증은
아직 완료하지 않았습니다.** [검증 범위](docs/windows-preview.md)를 확인하세요.
설치 화면과 확장은 영어이며 Mozilla 서명 확장 **0.4.2**를 포함합니다.
확장의 Settings에서 관리하며, 연결 재설정 중에도 업데이트·제거 결과를 표시합니다.
기존 제거·복구 파일 잠금 수정도 포함합니다.

1. 공식 배포처에서 Codex와 Firefox를 설치합니다.
2. [Windows beta.3 ZIP](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.5.0-beta.3/firefox-codex-mcp-0.5.0-beta.3-windows-x86_64.zip)을 받아 압축을 완전히 풀고 `firefox-codex-mcp.exe`를 더블클릭합니다. 관리자 권한으로 실행하지 않습니다.
3. 영어 관리 화면의 **Install connection → Continue**를 누릅니다.
4. **Add Codex Work Tabs to Firefox**를 누르고 Firefox의 확장 설치를 확인합니다.
5. 새 Codex 채팅에서 `https://example.com을 브라우저로 열고, Learn more 링크를 눌러 이동한 페이지 제목을 알려줘.`처럼 요청합니다.

코드·토큰·프로필 경로나 터미널 입력이 필요 없습니다. Mozilla 서명 확장 0.4.2를 포함합니다.
**Windows 연결 프로그램의 Authenticode 서명은 없습니다.** SmartScreen의
다운로드 설치 흐름은 미검증입니다. 보안 기능을 끄지 마세요. 차단된다면 서명 배포를
기다리거나 소스를 검토해 직접 빌드해야 합니다.

업데이트는 새 ZIP을 풀고 그 안의 EXE를 실행해 **Update connection**을 누릅니다.
이어서 **Add Codex Work Tabs to Firefox**로 함께 제공된 서명 확장 0.4.2를 설치합니다.
이전 작업 탭 접근은 해제되지만 결과 탭은 남습니다. 새 Codex 채팅에서 다시 시작하세요.
점검·복구·제거는 일반 Firefox의 **Codex Work Tabs → Settings**에서 합니다.
**Remove connection**을 누르면 제거됩니다. **Windows 설정 → 앱 → Firefox Codex MCP →
제거**도 사용할 수 있습니다. 관리 프로그램은 자동 종료하며, 실행 파일 잠금 해제를 위해 기존 Codex 채팅을 닫아야 합니다. Firefox 프로필·로그인·
결과 탭·무관한 설정은 보존합니다. 확장은 Firefox에서 별도로 제거합니다.
BiDi는 Linux 전용이며 WSL·Store Firefox·ARM64·관리자/다른 계정 혼용은 검증 범위 밖입니다.

## 일상적인 제어

| 도구 | 동작 |
|---|---|
| `firefox_status` | 연결된 확장과 이 채팅의 작업 탭 상태 |
| `firefox_open_tab` | HTTP(S) URL을 새 배경 작업 탭으로 열고 선택 |
| `firefox_tabs`, `firefox_select_tab` | 이 채팅이 만든 탭만 조회·선택 |
| `firefox_navigate` | 작업 탭에서 페이지 이동 |
| `firefox_snapshot` | 본문·제목·상태·컨트롤 관찰 |
| `firefox_click`, `firefox_type`, `firefox_scroll` | 최신 스냅샷으로 조작 |
| `firefox_close_tab` | 선택한 작업 탭 닫기 |
| `firefox_revoke` | 이 채팅의 제어권 해제; 결과 탭은 유지 |

- 사용자 기존 탭, 다른 채팅의 탭, 해제된 탭을 인수하지 않습니다. 채팅당 최대 8개.
- 확장 방식은 페이지가 연 팝업도 자동 인수하지 않습니다.
- 페이지 이동은 탭 소유권만 유지하며 새 스냅샷을 요구합니다.
- 같은 채팅은 다음 메시지에서도 동일한 작업 탭과 입력 상태를 이어 씁니다.
  MCP 프로세스가 교체돼도 Firefox와 라우터가 연결된 동안에는 유지됩니다.
  새 연결은 이전 연결의 명령·스냅샷을 무효화하므로 먼저 새 스냅샷을 읽습니다.
- 명시적 철회·확장 연결 손실·Firefox/연결 프로그램 재시작이나 업데이트·중지·프로필 전환은
  기존 접근을 영구 해제합니다. 결과 탭은 남지만 다시 인수할 수 없습니다.
- 24시간 동안 요청이 없으면 접근이 만료됩니다. 최대 128개 채팅의 상태를 메모리에 유지합니다.
- 앱이 전달하는 `threadId`가 없는 클라이언트는 기존 연결 단위 권한을 유지합니다.
  `firefox_status`의 `continuity`는 `chat` 또는 `connection`으로 범위를 알립니다.
- 채팅을 닫거나 보관하는 이벤트만으로 즉시 철회를 보장하지 않습니다.
  접근을 끝내려면 철회/중지 버튼을 사용하세요. [턴 간 연결 설계](docs/chat-continuity.md).
- 확장 버튼에서 **Pause**, **Revoke all tab access**를 사용할 수 있습니다.
- 스냅샷은 30초 이내 한 번의 조작에 사용합니다. 대상·주변 문맥·상태·모달·뷰포트 변경을
  검사하며 무관한 DOM 갱신은 허용합니다. 문서 식별자는 콘텐츠 포트에서 동기 검사합니다.

이 컴퓨터의 Codex 채팅은 해당 프로필의 로그인된 웹사이트를 새 탭으로 사용할 수 있습니다.
탭 분리는 MCP 기능의 논리적 경계이며 같은 OS 사용자로 실행되는 악성 프로그램을 막는
샌드박스는 아닙니다. 페이지는 명령이 아닌 비신뢰 데이터입니다. 본문이 Codex로 전달되는
범위는 [개인정보 안내](PRIVACY.md)를 확인하세요. iframe, 파일 업로드, 신뢰된 네이티브 입력은
지원하지 않습니다. 타임아웃이면 중복 제출하지 않도록 결과부터 확인하세요.
이미 브라우저로 전송된 조작은 철회로 되돌릴 수 없습니다.

## 업데이트·제거

RPM·DEB는 새 파일을 소프트웨어 관리자나 같은 dnf/apt 명령으로 설치해 업데이트합니다.
각 계정은 시스템 프로그램을 직접 사용합니다. 패키지가 바뀌거나 제거되면 이전 패키지의
실행 중인 연결 프로그램을 중단하고 접근을 해제합니다. Firefox와 결과 탭은 유지하며
업데이트 후 새 Codex 채팅에서 시작하세요. **Reconnect Codex**는 계정 설정 재연결이며
OS 파일을 업데이트하지 않습니다.

제거는 연결한 계정마다 **Codex Work Tabs → Settings → Disconnect Codex**를 누른 뒤
소프트웨어 관리자, `sudo dnf remove firefox-codex-mcp` 또는
`sudo apt remove firefox-codex-mcp`로 시스템 프로그램을 제거합니다. DEB purge도 지원합니다.
Disconnect는 그 계정의 소유한 연결·스킬·지침만 제거하고 다른 계정용 시스템 프로그램은
남깁니다. OS 제거는 사용자 홈을 편집하지 않습니다. 먼저 패키지를 제거했다면 재설치 후
Disconnect하거나 현재 tar.gz의 `./install.sh --uninstall`로 계정 장부를 정리하세요.

tar.gz 보조 배포는 새 압축 파일의 `install.sh` → **Update connection**으로 갱신하고,
확장의 **Settings → Remove connection** 또는 `./install.sh --uninstall`로 제거합니다.
RPM/DEB로 옮길 때는 패키지를 설치하고 **Reconnect Codex**를 선택합니다. 장부 소유이며
수정되지 않은 사용자 복사본·native-host·구형 실행기만 정리하고 수정·외부 파일은 보존합니다.
개인 Native Messaging 등록이 시스템 등록을 가리면 충돌을 알리고 이전을 중단합니다.

일반 Firefox 프로필·로그인·결과 탭과 사용자 수정은 보존합니다. 기존 채팅은 닫고 확장은
Firefox에서 별도로 제거하세요. 새 서명 XPI가 제공되면 별도로 설치합니다. 직접 배포 XPI의
자동 업데이트 채널과 자동 APT/DNF 저장소는 없습니다. RPM·DEB는 별도 패키지·저장소 서명이
없으며 Mozilla 서명은 XPI에 적용됩니다. 체크섬과 GitHub 빌드 증명을 함께 확인하세요.

관리 화면은 일반 Firefox의 **Codex Work Tabs → Settings**, 직접 실행은
`firefox-codex-mcp --manage`입니다. 설치 장부·복구 정보는
`~/.local/state/firefox-codex-mcp/`에 있습니다. [관리·복구 안내](docs/firefox-only-management.md).

```bash
firefox-codex-mcp --doctor
firefox-codex-mcp --repair
firefox-codex-mcp --uninstall   # 현재 계정 연결만 해제
# tar.gz 보조 배포는 위 실행 파일 대신 ./install.sh 사용
```

고급 BiDi는 `--install-bidi [프로필 절대 경로]`로 유지합니다. 디버깅용 실행기는 만들지
않습니다. 작업을 저장하고 Firefox를 닫은 뒤 `--launch-firefox /absolute/profile`을 사용합니다.
일반 확장 연결은 기존 Firefox만 실행하면 됩니다.

## 개발·검증·패키징

Rust 1.89+, Node 24+ 및 패키징용 `zip`, `unzip`, `tar`, `dpkg-deb`, `rpmbuild`가 필요합니다.
서명 확장 UI 시험과 패키징에는 현재 확장 소스와 일치하는 서명 XPI도 필요합니다.
[서명 상태](packaging/SIGNING.md)를 확인하세요.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test --test-isolation=none extension/tests/*.test.cjs
cargo build --release --locked
FIREFOX_SETUP_CODEX_BINARY=/path/to/codex cargo test --release --test install_e2e -- --ignored
FIREFOX_SIGNED_XPI="$PWD/packaging/signed/codex-work-tabs-0.4.2.xpi" FIREFOX_SIGNED_UI=1 FIREFOX_GECKODRIVER=/path/to/geckodriver cargo test --release --test firefox_e2e native_e2e:: -- --ignored --nocapture
cargo run --release --locked --bin package -- target/release/firefox-codex-mcp dist/release --linux-packages
```

실제 Firefox E2E는 격리된 HOME과 임시 프로필만 사용합니다. Firefox UI 시험에만
`--allow-system-access`와 `MOZ_REMOTE_ALLOW_SYSTEM_ACCESS=1`을 적용합니다.
위 명령은 서명된 XPI의 실제 설치 화면과 재시작을 검증합니다. `FIREFOX_SIGNED_XPI`를
생략하면 개발 확장을 사용하며 재시작 후 재로드가 필요합니다.
확장 구조는 [extension-mode.md](docs/extension-mode.md), 과거 시험은 [validation.md](docs/validation.md).
`manifest-legacy.json`과 구형 코드·토큰 UI는 회귀 시험용으로만 보존되며 배포 XPI에 포함되지 않습니다.

배포 파일에는 실행 파일, 설치 화면, XPI, MIT 라이선스, 의존성 라이선스, CycloneDX SBOM,
SHA256SUMS가 포함됩니다. 체크섬은 서명을 대신하지 않습니다. 공개 GitHub CI에서는 빌드
증명도 생성합니다. CI 아티팩트는 만료될 수 있습니다. 프로젝트 라이선스: [MIT](LICENSE).
