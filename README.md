# firefox-codex-mcp

Codex가 **현재 Firefox 프로필에 작업용 탭을 직접 열고 사용하는** 로컬 MCP 서버입니다.
일상적인 사용에 확장 설치, 연결 코드, 토큰 복사, 탭별 허용 버튼이 필요하지 않습니다.

```text
Codex 채팅 A ─ stdio MCP ─┐                         ┌─ A가 만든 작업 탭
                         ├─ Rust 로컬 broker ─ BiDi ┤
Codex 채팅 B ─ stdio MCP ─┘                         └─ B가 만든 작업 탭
```

Firefox의 WebDriver BiDi 디버깅 연결은 하나를 공유하고, 각 MCP 연결에 생성한 탭의
소유권을 배정합니다. 기본 프로필의 저장된 세션을 사용하며, 별도 컨테이너를 만들지 않습니다.
Codex의 **컴퓨터 사용 목록 통합은 보류**한 상태입니다. 앱 소스를 패치하지 않고 MCP 도구로 동작합니다.

## 사용 흐름

1. 설치기를 한 번 실행합니다. 사용 중인/기본 프로필을 탐지해 실행기·전역 MCP·자동 사용 지침을 등록합니다.
2. 이미 일반 모드로 실행 중인 Firefox는 저장 후 한 번 종료하고 **Firefox (Codex 연결)**로 엽니다.
3. 새 로컬 Codex 채팅에 “이 페이지를 열고 버튼을 확인해”라고 요청합니다. 브라우저가 필요한 작업이면 AI가 Firefox 도구를 선택할 수 있습니다. Firefox를 명시할 필요는 없습니다.

Firefox가 꺼져 있으면 `firefox_open_tab`이 지정된 프로필로 디버깅 모드를 시작합니다.
일반 Firefox 실행기로 먼저 열었다면 디버깅 옵션 적용을 위한 재시작이 필요합니다.
전역 등록은 이 컴퓨터의 로컬 Codex 채팅에 적용됩니다. 이미 열린 채팅은 새 채팅/앱 재시작이
필요할 수 있고, 프로젝트 설정·관리자 정책이 우선할 수 있습니다. 클라우드·다른 컴퓨터에는
별도 설치가 필요합니다. 자동 선택은 지침과 도구 설명으로 유도하며 모든 모델 판단을 강제하지 않습니다.
연결 문제는 `firefox_status`가 진단합니다. 기존 탭을 골라 넘기거나 비밀값을 복사하지 않습니다.

## 도구와 수명주기

| 도구 | 동작 |
|---|---|
| `firefox_status` | 디버깅 연결과 이 채팅의 작업 탭 상태 |
| `firefox_open_tab` | HTTP(S) URL을 새 작업 탭으로 열고 자동 선택 |
| `firefox_tabs`, `firefox_select_tab` | 이 채팅이 소유한 탭만 조회·선택 |
| `firefox_navigate` | 작업 탭에서 페이지 이동; 이후 새 스냅샷 필요 |
| `firefox_snapshot` | 보이는 본문·제목·상태·컨트롤 관찰 |
| `firefox_click`, `firefox_type`, `firefox_scroll` | 최신 스냅샷을 기준으로 조작 |
| `firefox_close_tab` | 선택한 작업 탭 닫기 |
| `firefox_revoke` | 이 채팅의 모든 탭 제어권 해제; 페이지는 남김 |

- 다른 채팅이나 사용자가 원래 열어 둔 탭을 자동으로 인수하지 않습니다.
- 새 작업 탭은 배경에서 열어 사용자가 보고 있는 탭의 포커스를 유지합니다.
- 같은 작업 탭의 페이지 이동은 소유권을 유지합니다. 탭을 닫으면 접근도 끝납니다.
- 원래 작업 탭을 opener로 확인할 수 있는 새 최상위 탭은 같은 채팅에 배정합니다.
- 채팅 연결 종료·디버깅 연결 손실·권한 해제 후에는 기존 탭을 재인수하지 않습니다.
  새 작업은 새 탭으로 시작합니다. 채팅당 최대 8개입니다.
- 스냅샷은 30초 이내, 한 동작에만 사용합니다. 대상·주변 문맥·상태·모달·뷰포트가
  바뀌면 다시 관찰하며, 무관한 DOM 갱신은 작업을 막지 않습니다.

## 설치·업데이트·제거 (Linux x86_64)

배포 패키지는 Rust·Node·확장 프로그램 없이 실행됩니다. 로컬 Codex 앱/CLI와 일반 Linux Firefox가 필요합니다.
GitHub의 **Linux distribution → Artifacts → firefox-codex-mcp-linux-x86_64**에서 내려받아 압축을 풉니다.
배포 CI는 Ubuntu 22.04에서 빌드합니다(glibc 2.35 이상). Windows/macOS와 Snap/Flatpak 실행은 아직 지원 검증하지 않았습니다.

1. `install.sh`를 실행합니다. 파일 관리자가 실행하지 않으면 폴더에서 터미널을 열어 `./install.sh`를 입력합니다.
2. 관리 화면에서 프로필 확인 → **설치 → 진행**을 누릅니다.
3. 일반 모드 Firefox가 켜져 있으면 저장 후 종료하고 **Firefox (Codex 연결)**로 한 번 다시 엽니다. 새 Codex 채팅에서 웹 작업을 요청합니다.

관리 화면을 여는 데 토큰 복사나 관리자 권한은 필요 없습니다. 앱 메뉴의 **Firefox Codex 관리**에서
진단·중단된 설치 복구·제거를 실행할 수 있습니다. 업데이트는 **새 패키지의 install.sh → 업데이트**입니다.
실행 파일은 원자적으로 교체하며 Firefox나 채팅을 강제 종료하지 않습니다. 새 버전을 쓰려면 기존 Firefox 도구
채팅을 종료하고 10초 뒤 새 채팅에서 시작하세요. 다른 IPC 버전의 브로커에는 제어 요청을 보내지 않습니다.

설치 장부는 `~/.local/state/firefox-codex-mcp/`에 저장합니다. 공식 Codex CLI로 준비한 설정을
다른 설치 파일과 함께 적용하고 실패하면 복구합니다. 다른 MCP, 사용자 지침과 Firefox 데이터를 보존합니다.
제거는 설치한 파일/관리 블록만 대상으로 하며, 설치 후 사용자가 수정한 파일은 보존하거나 해당 블록만 제거합니다.
실행 중인 채팅/Firefox의 접근은 제거만으로 즉시 중단되지 않습니다. 종료 후 일반 Firefox로 다시 여세요.
`prefs.js`에 남은 `remote.prefs.recommended=false`는 전체 파일을 덮어쓰지 않기 위해 유지할 수 있습니다.

```bash
./install.sh --install [프로필_절대경로]  # 그래픽 화면 없는 설치
./install.sh --update
./install.sh --doctor
./install.sh --uninstall
./install.sh --repair
```

상세 안내와 복구 절차는 [INSTALL.txt](packaging/INSTALL.txt)를 참고하세요. 프로젝트 라이선스는 [MIT](LICENSE)입니다.
배포 묶음에는 SHA256SUMS, 의존성 라이선스 원문, CycloneDX SBOM이 포함됩니다. 체크섬은 배포자 서명이 아닙니다.
공개 저장소 CI에서는 빌드 증명을 추가하고, 비공개 저장소에서는 이 단계를 건너뜁니다. CI 아티팩트는 만료될 수 있으며
상시 공개 다운로드용 GitHub Release 게시 및 저장소 공개 전환은 별도 배포 단계입니다.

소스에서 개발할 때만 Rust 1.89 이상과 테스트용 Node.js 24 이상이 필요합니다.

```bash
cargo build --release --locked
./target/release/firefox-codex-mcp --manage
cargo run --release --locked --bin package -- target/release/firefox-codex-mcp dist
```

자동 설치 대신 설정을 직접 관리할 때의 예시는 다음과 같습니다.

```toml
[mcp_servers.firefox]
command = "/home/USER/.local/lib/firefox-codex-mcp/firefox-codex-mcp"
args = ["--bidi"]
startup_timeout_sec = 20
tool_timeout_sec = 70

[mcp_servers.firefox.env]
FIREFOX_MCP_PROFILE = "/absolute/path/to/current-firefox-profile"
```

Firefox가 생성한 `WebDriverBiDiServer.json`에서 동적 포트를 발견합니다.
테스트나 수동 디버깅에는 `FIREFOX_MCP_BIDI_URL=ws://127.0.0.1:PORT/session`을 사용할 수 있습니다.
일상적인 설정에는 포트 번호를 복사하지 않습니다.

## 검증

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test --test-isolation=none extension/tests/*.test.cjs
cargo test --test firefox_e2e debug_broker_e2e -- --ignored --nocapture
cargo test --test firefox_e2e launcher_lifetime_e2e -- --ignored --nocapture
```

브라우저 테스트는 임시 프로필의 실제 Firefox를 띄워 자동 탭 생성, 읽기·입력, 두 채팅의
격리, 기존 탭 보존, 권한 해제, 재연결, 프로필 저장 데이터 공유를 검사합니다.
확장·geckodriver·시스템 접근 옵션을 사용하지 않습니다.
실행기 종료 뒤 Firefox 유지와 오래된 디버깅 포트 파일 교체도 검사합니다.
세부 결과는 [검증 기록](docs/validation.md), 설계는 [쉬운 연결 설계](docs/easy-connection-design.md)에 있습니다.

## 범위와 한계

탭 격리는 **우리 broker가 강제하는 MCP 도구의 경계**입니다. Firefox 디버깅 자체가
채팅별 보안 샌드박스를 제공하는 것은 아닙니다. 디버깅 포트는 `127.0.0.1`로만 연결하며,
broker 통신은 사용자 전용 Unix socket으로 제한합니다. 시스템 UI 접근은 요청하지 않습니다.

페이지의 본문·라벨·URL에 있는 개인정보는 Codex로 전달될 수 있습니다. 입력 버퍼와
비밀번호/파일 입력은 관찰에서 제외합니다. 페이지 내용은 명령으로 신뢰하지 않아야 하며,
전송·결제·삭제 등 중요한 동작에 필요한 사용자 승인은 별개입니다.

클릭·입력은 현재 DOM 방식입니다. 일부 사이트는 신뢰된 입력 이벤트를 요구합니다.
iframe/shadow DOM 전체, 풍부한 편집기, select 옵션, 파일 전송, 다운로드는 아직 지원하지 않습니다.
연결 손실이나 시간 초과 뒤에는 결과를 확인하기 전 같은 동작을 반복하면 안 됩니다.
정상 broker 종료는 디버깅 세션을 정리하지만, 강제 종료(SIGKILL)·충돌로 세션이 남으면
저장 후 Firefox 재시작이 필요할 수 있습니다. `--doctor`의 포트 도달 여부는 세션 준비 완료와 다릅니다.
0.2.0은 Linux용 설치·업데이트·제거와 실제 Firefox 제어를 검증한 배포 후보입니다. 모든 배포판/사이트에서 검증된 것은 아닙니다.

예전 확장 방식은 [확장 연결 문서](docs/extension-mode.md)에 남겨 두었습니다.
[Foxhand·앱 통합 비교](docs/codex-integration.md)는 그 방식의 검토 기록입니다.
현재는 Mozilla 공식 [Firefox DevTools MCP](https://github.com/mozilla/firefox-devtools-mcp)와도 비교했으며,
이 프로젝트의 추가 가치는 **동시 채팅의 작업 탭 소유권과 기존 관찰·동작 검증**입니다.
공식 MCP의 전체 DevTools 기능을 재구현하려는 목적은 아닙니다.

## 제품 완료 기준

- 전역 도구 발견: 프로젝트 밖 실제 로컬 Codex 채팅에서도 Firefox 도구가 보일 것.
- 자연스러운 선택: 브라우저 이름을 생략한 화면 확인/조작 요청에서 도구를 선택할 것.
- 간편 설치: 경로·토큰·포트 입력 없이 탐지/등록/진단을 끝내고 재설치가 안전할 것.
- 일반 배포: 깨끗한 지원 OS의 설치/업데이트/제거와 그래픽 설치 창 검증, 배포자 신뢰 경로,
  공개 다운로드, 라이선스를 정할 것. 앞의 개발 검증만으로 이 단계까지 완료했다고 보지 않습니다.

현재 설치/관리 UI와 임시 사용자 E2E는 통과했습니다. 상시 공개 다운로드 및 더 넓은 OS 검증은 별도입니다.
