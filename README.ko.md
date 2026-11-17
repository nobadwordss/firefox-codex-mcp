# Firefox Codex MCP

제품의 초점은 Codex 채팅별 탭 격리·이어쓰기·철회와 쉬운 설치입니다.
관련 도구와 [동일 조건 비교 계획](docs/comparison-plan.md)을 공개하며,
아직 직접 측정하지 않은 성능·보안 우위를 주장하지 않습니다.

[English](README.md) · **한국어**

![Codex Work Tabs 커서](extension/icons/cursor-96.png)

Codex가 사용 중인 Firefox의 로그인 상태로 **자신의 작업 탭을 직접 열고 사용하는** 로컬 MCP입니다.
기본 연결은 Firefox 확장 + 로컬 연결 프로그램입니다. 코드·토큰·프로필 경로를 입력하거나
탭마다 허용할 필요가 없습니다. 기존 WebDriver BiDi 방식은 고급 옵션으로 유지합니다.

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

1. [v0.4.0 배포 패키지](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.0)를 받아 압축을 풀고 `install.sh` 실행 → **설치 → 진행**.
2. 같은 관리 화면에서 **Firefox에 Codex Work Tabs 확장 추가**를 누르고 Firefox의 설치·권한 안내를 확인합니다.
3. 새 Codex 채팅에 웹 작업을 요청합니다. AI가 작업 탭을 직접 엽니다.

**0.4.0 연결 프로그램 + 확장:** Mozilla가 서명한 `codex-work-tabs-0.4.0.xpi`를 패키지와 관리 화면에 포함합니다.
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
여러 프로필이 처음 연결되면 원하는 Firefox 확장에서 **이 Firefox 사용**을 한 번 누릅니다.
선택한 프로필이 꺼져 있으면 다른 프로필로 자동 전환하지 않습니다.
Firefox가 모두 꺼졌을 때 일반 Firefox 실행을 시도합니다. Firefox 자체의 기본 프로필이
선택한 프로필과 다르면 해당 프로필을 직접 열어야 합니다. 경로를 추측하지 않습니다.

전역 MCP·스킬·짧은 사용 지침을 설치해 브라우저가 필요한 작업에서 AI가 Firefox를 고려하도록
합니다. 모든 모델의 도구 선택을 강제하는 기능은 아닙니다. 이미 열린 채팅은 새 채팅/앱 재시작이
필요할 수 있고 프로젝트 설정·관리자 정책이 우선합니다. 클라우드 채팅에는 적용되지 않습니다.
Codex의 **컴퓨터 사용 목록**을 패치하지 않고 MCP로 연결합니다.

## Windows 프리뷰

[v0.5.0-beta.1](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.5.0-beta.1)은
Windows 10 22H2·Windows 11 x64와 일반 데스크톱 Firefox를 대상으로 합니다.
자동 검증 환경은 Windows Server 2022이며, **Windows 10·11 데스크톱에서의 실제 검증은
아직 완료하지 않았습니다.** [검증 범위](docs/windows-preview.md)를 확인하세요.

1. 공식 배포처에서 Codex와 Firefox를 설치합니다.
2. Windows ZIP을 받아 압축을 풀고 `firefox-codex-mcp.exe`를 더블클릭합니다. 관리자 권한으로 실행하지 않습니다.
3. 영어 관리 화면의 **Install connection → Continue**를 누릅니다.
4. **Add Codex Work Tabs to Firefox**를 누르고 Firefox의 확장 설치를 확인합니다.
5. 새 Codex 채팅에서 브라우저 작업을 요청합니다.

코드·토큰·프로필 경로나 터미널 입력이 필요 없습니다. 확장은 기존 Mozilla 서명 0.4.0을
사용하지만 **Windows 연결 프로그램의 Authenticode 서명은 없습니다.** SmartScreen의
다운로드 설치 흐름은 미검증입니다. 보안 기능을 끄지 마세요. 차단된다면 서명 배포를
기다리거나 소스를 검토해 직접 빌드해야 합니다.

업데이트는 새 ZIP을 풀고 그 안의 EXE를 실행합니다. 제거는 **Windows 설정 → 앱 → Firefox Codex MCP →
제거**에서 관리 화면을 연 뒤 **Remove connection**을 누릅니다. 관리 프로그램은 자동
종료하며, 실행 파일 잠금 해제를 위해 기존 Codex 채팅을 닫아야 합니다. Firefox 프로필·로그인·
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
- 확장 버튼에서 **잠시 중지**, **모든 작업 탭 접근 해제**를 사용할 수 있습니다.
- 스냅샷은 30초 이내 한 번의 조작에 사용합니다. 대상·주변 문맥·상태·모달·뷰포트 변경을
  검사하며 무관한 DOM 갱신은 허용합니다. 문서 식별자는 콘텐츠 포트에서 동기 검사합니다.

이 컴퓨터의 Codex 채팅은 해당 프로필의 로그인된 웹사이트를 새 탭으로 사용할 수 있습니다.
탭 분리는 MCP 기능의 논리적 경계이며 같은 OS 사용자로 실행되는 악성 프로그램을 막는
샌드박스는 아닙니다. 페이지는 명령이 아닌 비신뢰 데이터입니다. 본문이 Codex로 전달되는
범위는 [개인정보 안내](PRIVACY.md)를 확인하세요. iframe, 파일 업로드, 신뢰된 네이티브 입력은
지원하지 않습니다. 타임아웃이면 중복 제출하지 않도록 결과부터 확인하세요.
이미 브라우저로 전송된 조작은 철회로 되돌릴 수 없습니다.

## 업데이트·제거

새 패키지의 `install.sh`에서 업데이트합니다. 실행 중인 Firefox를 종료하지 않습니다.
확장 연결을 갱신하며 이전 작업 탭 접근은 해제됩니다. 새 Codex 채팅에서 다시 시작하세요.
확장은 Firefox 부가 기능 관리자에서 업데이트/제거합니다. 직접 배포한 서명 XPI에는 별도의
자동 업데이트 URL이 없으므로 새 서명 XPI 설치가 필요합니다. AMO 목록 배포는 추후 단계입니다.

앱 메뉴의 **Firefox Codex 관리**에서 진단·복구·제거할 수 있습니다. 제거는 전역 MCP 등록과
설치한 파일/관리 블록만 대상으로 하며 사용자 수정, Firefox 프로필·로그인·결과 탭은 보존합니다.
기존 채팅을 종료하면 남아 있는 연결도 끝납니다. 확장은 별도로 제거하세요.
설치 장부와 복구 정보는 `~/.local/state/firefox-codex-mcp/`입니다.

```bash
./install.sh --install          # 확장 연결 기본 설치: 프로필 불필요
./install.sh --update           # 설치된 연결 방식 유지
./install.sh --doctor
./install.sh --uninstall
./install.sh --repair
./install.sh --install-bidi     # 고급: 디버깅 연결과 프로필 자동 탐지
./install.sh --install-bidi /absolute/profile
```

관리 화면 기본 선택은 확장입니다. 기존 BiDi 설치를 확장으로 바꾸려면 기본 설치를 실행하세요.
기존 BiDi 실행기/설정의 설치 장부는 유지해 이후 제거할 수 있습니다. BiDi를 쓰는 경우에만
저장 후 **Firefox (Codex 연결)**로 실행해야 합니다. 앱을 강제 종료하지 않습니다.

## 개발·검증·패키징

Rust 1.89+, Node 24+ 및 패키징용 `zip`, `unzip`, `tar`가 필요합니다.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test --test-isolation=none extension/tests/*.test.cjs
cargo build --release --locked
FIREFOX_SETUP_CODEX_BINARY=/path/to/codex cargo test --release --test install_e2e -- --ignored
FIREFOX_SIGNED_XPI="$PWD/packaging/signed/codex-work-tabs-0.4.0.xpi" FIREFOX_SIGNED_UI=1 FIREFOX_GECKODRIVER=/path/to/geckodriver cargo test --release --test firefox_e2e native_e2e:: -- --ignored --nocapture
cargo run --release --locked --bin package -- target/release/firefox-codex-mcp dist/release
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
