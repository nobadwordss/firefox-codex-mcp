# 검증 기록

Linux 첫 설치: [공개 0.4.0 배포 파일·빈 홈·실제 모델의 첫 웹 작업](linux-first-install-e2e-2026-11-17.json).
기존 설정은 가리고 로그인만 읽기 전용으로 연결했습니다. 새 OS나 초보 사용자 사용성
시험과는 구분하며, [처음 설치 안내](first-install-linux.md)에 실행·승인·첫 작업 단계를 보완했습니다.

Windows 0.5.0-beta.1: [Windows Server 실제 설치·Codex–Firefox E2E·배포 ZIP 검증](windows-e2e-2026-11-17.json).
Windows 10/11 데스크톱·실제 모델의 자동 도구 선택·SmartScreen 검증은 아직 남아 있습니다.

최신 0.4.0: [채팅 연속성·실제 데스크톱·서명 확장 검증](chat-continuity-e2e-2026-11-17.json).
아래는 이전 버전의 역사적 검증이며 현재 기본 연결 방식은 Native Messaging 확장입니다.

2026-11-16, Fedora 45 / Firefox 160.0 / geckodriver 0.37.1 기준입니다.

## 0.2.0 설치·업데이트·제거 제품화

빈 임시 HOME/CODEX_HOME과 임시 Firefox 프로필에서 실제 관리 UI를 조작했습니다.
사용자의 원래 Firefox 프로필은 이 검증에서 변경하지 않았습니다.

- UI 설치 → 공식 Codex CLI 전역 등록 → 설치된 실행 파일의 실제 Firefox E2E 통과.
- Firefox 두 MCP 연결의 자동 작업 탭, 읽기·클릭·입력, 기존 탭/프로필 저장 데이터 보존,
  채팅 간 격리, 철회·재연결 후 재인수 거절 검증.
- UI 업데이트 → 연결 진단 → 제거 완료. MCP·스킬·실행기·바이너리 제거와 기존 설정/지침/user.js 원문 복원 확인.
- 설치 뒤 사용자 편집을 추가한 상태에서 업데이트→제거해도 그 편집은 보존하는 회귀 테스트 통과.
- 중단된 트랜잭션 복구, 충돌한 외부 편집 보존, 심볼릭 링크 거부 검사.
- 익명 및 다른 Origin에서 보내는 관리 API 제거 요청은 403으로 거부.
- 관리 화면 증거: `/tmp/firefox-codex-manager-removal.jpg` (로컬 테스트 산출물).
- 이번 검증은 실제 Firefox + 공식 Codex 설정 CLI + stdio MCP 하네스입니다.
  모델이 직접 도구를 선택한 데스크톱 채팅 검증은 아래 이전 단계 결과와 구분합니다.

배포: MIT 승인, 버전 tar.gz/체크섬/생산·빌드 의존성 라이선스 원문/CycloneDX SBOM 생성.
GitHub 원격 CI 결과는 제품화 검증 파일에 기록합니다. 비공개 저장소는 공개로 바꾸지 않았습니다.

## 이전 단계: 전역 등록과 자동 선택 개선

2026-11-16, `--install`로 사용자 전역 MCP·프로필 탐지·사용 지침·스킬 설치를 통합했습니다.
실제 Codex CLI를 임시 HOME/CODEX_HOME으로 실행한 설치 E2E에서 설정/주석/기존 지침 보존,
설치된 바이너리로 반복 설치, 별도 `firefox` MCP 보호와 `--doctor` 진단이 통과했습니다.
기존 사용자 프로필은 자동으로 올바르게 탐지됐으며 브라우저 재시작 없이 설치했습니다.

프로젝트 없는 실제 데스크톱 채팅 `01a1e44d-d7fd-7e11-b369-d4f7542f642f`에 Firefox/MCP를
언급하지 않고 localhost 페이지의 읽기·클릭·입력을 요청했습니다. 채팅이 `firefox-browser`
스킬을 읽고 Firefox MCP를 선택했으므로 **전역 발견 및 자연어 자동 선택은 확인**했습니다.
첫 실제 작업은 이전 버전이 남긴 BiDi 세션으로 `Maximum number of active sessions`
오류가 발생해 완료하지 못했습니다. TCP 포트 도달만으로 세션 사용 가능을 주장하지 않습니다.
사용자 저장 확인과 재시작 승인을 받아 같은 프로필로 다시 실행한 뒤, 프로젝트 밖의
**독립된 새 데스크톱 채팅 두 개**에서 재검증했습니다. 두 프롬프트 모두 Firefox/MCP 이름을
넣지 않았고 설치/설정 변경 없이 브라우저 작업만 요청했습니다.

- 로컬 fixture: 코드 `009c28fb` 관찰, Count 0 → 1, `Count updated to 1` 확인,
  `전역 자동 연결: 009c28fb` 입력 도구 성공. 입력값의 독립 DOM readback은 하지 않았습니다.
- 공개 웹: `example.com`의 설명/`Learn more` 링크를 읽고 실제 클릭하여
  `https://www.iana.org/help/example-domains`, 제목 `Example Domains`를 확인했습니다.
- 두 채팅 모두 자동으로 Firefox 스킬/도구를 선택했습니다. 결과 탭은 남기고 revoke한 뒤
  status가 연결 유지·소유 탭 0을 반환했고, snapshot을 한 번씩 시도해 `No AI work tab` 거절을 확인했습니다.
- 재검증 MCP **16회: 성공 14회, 예상한 읽기 거절 2회, 예기치 않은 오류 0회**입니다.

증거는 [global-install-e2e-2026-11-16.json](global-install-e2e-2026-11-16.json),
실제 도구 원문은 `/tmp/firefox-codex-global-discovery-e2e/retest-mcp-trace.json`에 있습니다.
외부 웹 조작 한 건까지 자동 선택 근거를 확장했지만 모든 모델/사이트/클라우드 채팅의 동작을
보장하는 것은 아닙니다.
완료된 테스트의 MCP/fixture를 종료한 뒤 broker가 idle 종료했고 사용자 Firefox는 계속
실행 중임을 확인했습니다. 새 WebSocket에서 읽기 전용 `session.status`만 호출한 결과
`ready=true`여서 **정리 후 고아 세션 없이 다음 새 연결을 받을 수 있음**도 실측했습니다.
이 확인은 새 제어 세션을 생성하거나 기존 탭을 읽지 않았습니다. 증거는 같은 산출물 폴더의
`final-session-status.json`입니다.

이를 계기로 broker의 SIGTERM/SIGINT 처리와 구독 실패 시 `session.end` 정리를 추가했습니다.
`broker_termination_releases_firefox_session`은 실제 임시 Firefox에서 SIGTERM으로 broker를
종료한 뒤 **같은 Firefox가 새 세션을 수락**하는지 검사했고 통과했습니다(1.10초).
`debug_broker_e2e` 전체도 다시 통과했습니다(13.16초).
Rust 단위 14개+프로세스 1개, clippy, 설치 E2E, 스킬 검증, shell 구문 검사도 통과했습니다.
기존 JS는 변경하지 않아 앞의 53개 결과를 재사용합니다. GUI 설치 창과 원격 패키지 workflow는
실행 검증하지 않았습니다. 로컬 배포 묶음만 생성했고 공개 릴리스·서명은 하지 않았습니다.

SIGKILL·브로커 충돌처럼 종료 처리가 실행될 수 없는 경우 Firefox 재시작이 여전히 필요할 수
있습니다. 순수 BiDi 세션은 새 연결에서 세션 ID만으로 안전하게 회수할 수 있다고 보장하지
않습니다. 다른 디버거의 세션을 강제 종료하거나 사용자 작업 중인 Firefox를 자동 종료하지 않습니다.

## 현재 기본 방식: 직접 BiDi 연결

2026-11-16, Firefox 160.0 / Codex 데스크톱 26.1111.21308에서 검증했습니다.
사용자 승인 후 **현재 사용 중인 프로필**을 디버깅 모드로 재시작했습니다.
확장 설치, 연결 코드·토큰 복사, 수동 탭 허용, geckodriver, 시스템 접근 옵션은 사용하지 않았습니다.

### 실제 데스크톱 두 채팅

실제 Codex 데스크톱 A/B 채팅의 MCP 호출 **46회**를 원문과 대조했습니다.
그중 의도한 접근 거절은 5회입니다. 최초 실행기가 끝나면서 Firefox도 종료되는 문제가
있어 프로세스 그룹을 분리한 뒤 다시 검증했습니다. 최초의 미연결 status도 46회에 포함합니다.

| 확인 항목 | 결과 |
| --- | --- |
| 자동 작업 탭 | A/B가 각자 새 탭 생성, 본문 코드 관찰, 클릭·입력 성공 |
| 채팅 간 접근 | A → B, B → A tabId 선택 모두 거절; 탭 목록도 각자 소유 탭만 반환 |
| 권한 해제 | A 해제 후 읽기 및 옛 탭 재선택 거절 |
| 독립 제어 | A 해제 후에도 B가 Count 1 → 2로 계속 조작 |
| 다시 작업 | A가 새 작업 탭 생성, 페이지 이동 후 새 본문 코드 관찰·클릭·입력 |
| 종료 정리 | 모든 테스트 소유권 해제; 전용 MCP·idle broker·fixture 종료; 사용자 Firefox 계속 실행 |

A의 첫 본문 코드는 `6e52efe7`, B는 `1e9bf830`, A의 새 페이지는 `233ec9a5`였습니다.
현재 프로필 검증에서 입력 도구의 `typed=true`는 확인했지만, snapshot은 입력 버퍼를
제외하므로 최종 입력값의 독립 DOM 대조까지 했다고 주장하지 않습니다. 그 대조는 아래
임시 프로필의 실제 Firefox E2E에서 수행했습니다. 사용자 기존 탭의 내용을 수집하지 않았습니다.

정리된 증거: [bidi-current-profile-e2e-2026-11-16.json](bidi-current-profile-e2e-2026-11-16.json).
실제 도구 호출 원문: `/tmp/firefox-codex-current-bidi-check/mcp-trace.json`.
설치 실행 파일과 최종 release 빌드의 SHA-256 일치, 같은 프로필의 Firefox 실행 인자와
loopback 디버깅 listener 유지를 확인했습니다. 새 설정은 이 프로젝트의 새 Codex 채팅에 적용됩니다.

### 자동 검사와 실제 Firefox 회귀

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --offline -- -D warnings` 통과.
- `cargo test --workspace --offline`: 단위 12개 + 프로세스 연동 1개 통과.
  실브라우저/fixture 5개는 기본 실행에서 의도적으로 ignored이며 필요한 항목을 따로 실행했습니다.
- Node 24.22.3: `node --test --test-isolation=none extension/tests/*.test.cjs` 53개 통과.
- `debug_broker_e2e` 실제 Firefox 테스트 통과: 두 MCP 연결, 프로필의 동적 포트 발견,
  본문·상태 읽기, 클릭·입력 DOM 대조, 입력 버퍼 제외, 오래된 snapshot 거절,
  기존 탭 보존, profile localStorage 공유, 교차 접근 차단, 철회·재연결,
  수동 허용 없는 이동, opener로 연 자식 탭, 탭 닫기, idle 세션 종료를 검사했습니다.
  첫 MCP의 **전체 프로세스 그룹 종료 뒤에도 B의 제어가 유지**됐습니다.
  결과는 `/tmp/firefox-codex-bidi-e2e5/result.json`에 있습니다.
- `launcher_lifetime_e2e`는 임시 프로필에서 오래된 포트 파일이 새 연결로 교체되고,
  실행기의 프로세스 그룹이 끝난 뒤에도 Firefox가 BiDi 요청에 응답함을 검사합니다.

```bash
cargo test --test firefox_e2e debug_broker_e2e -- --ignored --nocapture
cargo test --test firefox_e2e launcher_lifetime_e2e -- --ignored --nocapture
```

BiDi 회귀는 확장이나 geckodriver 없이 실제 Firefox를 사용합니다. 공통 관찰 엔진 분리 후
확장 JS 회귀와 BiDi 실브라우저 검증을 수행했으며, 아래의 예전 확장 UI/CLI E2E 전체를
이번 BiDi 전환 후 다시 실행한 것은 아닙니다. 원격 CI, 다른 OS/브라우저 버전,
모든 로그인 사이트·편집기·iframe 동작을 검증한 것도 아닙니다.
채팅별 제한은 broker의 논리적 경계이며 Firefox 자체의 보안 격리가 아닙니다.

## 이전 확장 방식: 사용 중인 실제 Firefox 프로필

임시 프로필 E2E에 이어, 사용자가 평소 쓰는 Firefox에 개발용 확장을 직접 등록하고
전용 데스크톱 채팅 `01a1e407-0964-76d3-aef7-d58c01f754bc`와 연결했습니다.
이 검증에서는 WebDriver나 별도 Firefox 프로세스를 실행하지 않았습니다.
사용자가 `http://localhost:39351/` 테스트 탭만 명시적으로 허용했습니다.

실제 데스크톱 MCP 8회(status 3 / snapshot 3 / click 1 / type 1)가 성공했습니다.
처음 두 status는 정상적인 미연결 상태였고, 최종 status는 연결·탭 권한 모두 true였습니다.
본문 코드 `28b01811`을 읽고 Count 0을 한 번 클릭해 `Count updated to 1`을 관찰했으며,
Message에 `Codex 확인: 28b01811`을 입력했습니다. 입력값은 snapshot에서 제외되므로
type 응답만으로 화면 일치를 주장하지 않았고, 사용자가 실제 버튼과 입력값 모두
맞는다고 별도로 확인했습니다.

작은 팝업의 입력 유실과 자동 갱신에 덮이는 오류를 수정했고, 사용자는 확장 설정
화면을 일반 Firefox 탭으로 열어 연결을 완료했습니다. 실제 페이지 제어는 허용한
테스트 탭에서만 수행했습니다. 이 결과는 서명된 영구 설치, 다른 프로젝트 설정,
현재 원래 채팅의 도구 추가나 Firefox 재시작 후 자동 연결을 검증한 것은 아닙니다.

기록: [current-profile-e2e-2026-11-16.json](current-profile-e2e-2026-11-16.json).
설정과 남은 제품화 항목: [현재 프로필 설정](current-profile-setup.md).

## 이전 확장 방식: 실제 데스크톱 다중 채팅 E2E

Codex 데스크톱 **26.1111.21308**의 같은 프로젝트에서 A/B 두 채팅과 서버 종료 후
재연결용 C 채팅을 실제 생성했습니다. **18개 턴, MCP 호출 54회: 성공 42회,
의도한 거절 12회, 예기치 않은 MCP 오류 0회**입니다. 도구별로 status 14회,
snapshot 27회, click 6회, type 3회, scroll 4회입니다.
데스크톱 채팅이 도구를 호출했고 별도 WebDriver가 임시 Firefox의 툴바와 결과 DOM을
관찰했습니다. CLI 실행으로 데스크톱 검증을 대신하지 않았습니다.

| 검증 | 실제 결과 |
| --- | --- |
| 동시 시작 | A/B의 MCP PID, 동적 포트, clientId가 서로 다름. 기존 8765 충돌 없음 |
| 동시 읽기·제어 | A/B 턴이 27초 겹침. 각자 본문 코드를 읽고 클릭·입력, 각 탭 Count 1 |
| 탭 소유권 | B가 A 탭을 Enable하려는 시도는 실제 팝업에서 거절, A 권한 유지 |
| 교차 snapshot | B가 발급한 지 10.851초 된 ID로 A의 scroll(0) 호출 거절. 자신의 /a 내용 유지 |
| A만 철회 | A 연결 유지, 읽기·조작 거절. B는 /b에서 Count 1 → 2 |
| A 서버 종료 | A를 재허용 후 해당 PID만 종료. B는 Count 2 → 3으로 계속 제어 |
| 새 C 연결 | 다른 포트/ID. Connect만으로 이전 A 권한 상속 안 됨. 명시적 Enable 후 /a 읽기·입력·클릭 |
| B만 철회 | B 읽기·조작 거절. C는 /a에서 Count 2 → 3으로 계속 제어 |
| 전체 해제 | Disconnect all 후 B/C 모두 연결·권한 false, 읽기 거절 |

독립 DOM 관찰에서 처음 입력은 /a의 `A:56159f95`, /b의 `B:66faa6c4`였고,
최종 /a는 `C:56159f95`/Count 3, /b는 `B:66faa6c4`/Count 3이었습니다.
B는 앞선 교차 참조 요청을 도구 실행 전에 거절했으므로 이를 서버의 차단 결과로
계산하지 않았습니다. 양방향 새 snapshot의 혼용 거절은 JS 회귀에서도 검사합니다.
MCP 출력에서 비밀 토큰과 fixture의 private/hidden 표식 15종이 없음을 확인했습니다.

하네스는 821.48초에 종료됐습니다. 변경 후 **실제 Firefox + Codex CLI 전체 회귀도
36.41초에 통과**했습니다(도구 성공 9회, 허용 전 snapshot 거절 1회). 첫 회귀 실행은
CLI 실행 파일 경로를 잘못 지정해 Firefox 검사 뒤 중단됐으며, 설치 경로를 확인하고
재실행해 통과했습니다. 상세 CLI 산출물은 `/tmp/firefox-codex-multi-regression/`입니다.

정리된 증거는 [multi-chat-e2e-2026-11-16.json](multi-chat-e2e-2026-11-16.json),
채팅/MCP 원문·팝업 상태·DOM·스크린샷은 `/tmp/firefox-codex-multi-e2e/`에 있습니다.
테스트 MCP·geckodriver 종료, 임시 Firefox 프로필 삭제, 해당 테스트 포트 폐쇄를
확인했습니다. 개인 프로필은 사용하지 않았습니다.

보장 단위는 **stdio MCP 세션**이며 현재 앱에서 채팅별 프로세스 분리를 실측했습니다.
미래 클라이언트의 프로세스 공유, 앱 전체 재시작, OS 샌드박스 격리나 모든 사이트의
동시 제어 성능을 검증한 것은 아닙니다. 설계·수동 연결 방법과 경계는
[동시 채팅 설계](multi-chat-design.md)에 기록했습니다.

## 이전 확장 방식의 회귀 검사 기록

- Rust stable 1.100.0: `cargo fmt --all -- --check` 통과.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` 통과.
- `cargo test --workspace --locked`: Rust 단위 7개 + 프로세스 연동 1개 통과.
  Origin/token/인자 검증, MCP 초기화, 연결 없음 오류, 상호 인증 WebSocket 왕복,
  HMAC의 역할·clientId·challenge 분리, 이전 연결의 큐 명령 거절 및 세 동시 stdio
  프로세스의 서로 다른 포트·ID를 포함합니다.
- Node 24.22.3: `node --test --test-isolation=none extension/tests/*.test.cjs`:
  53개 통과. 파일 단위 결과만 보지 말고 총 테스트 수를 확인합니다.
- 실제 브라우저 테스트는 기본 `cargo test`에서는 ignored이며 아래처럼 별도 실행합니다.

확장 회귀는 연결과 탭 허용의 상태 분리, 독립 Revoke, 허용/명령 처리의 비동기 API 대기 중 Disconnect, 이전 허용 작업의
뒤늦은 완료, 같은 문서의 재허용, 이전 포트의 지연 이벤트, pending 탭 이동/닫힘,
명령 중복, 응답 유실/타임아웃, pagehide/pageshow를 검증합니다.
응답을 잃었지만 실제 클릭은 일어난 경우에도 즉시 unknown 오류를 반환하고
다음 명령이 잠금에 갇히지 않는지 확인합니다.
DOM 변경만으로는 안정된 대상을 거절하지 않는지, 라벨·가림막·disabled/busy·노드 제거·
위치·30초 만료·스크롤·사용자 입력 변경을 실행 전에 거절하는지도 검사합니다.
select는 관찰되지만 지원하지 않는 네이티브 선택 동작을 성공으로 반환하지 않습니다.
다중 연결 회귀는 양방향 snapshot 혼용 거절, 선택/철회/종료의 독립성, pending 탭 예약
경쟁, 소유자만의 이동/연결 유실, 잘못된 서버 ID·증명 및 비동기 증명 완료 전에 도착한
명령의 대기/거절을 검증합니다.
현재 프로필의 수동 연결을 진행하며 발견한 팝업 입력 유실도 수정했습니다. 공개 연결
코드는 팝업을 닫았다 다시 열어도 유지하고, 제출하지 않은 토큰 입력은 저장하지
않습니다. 잘못된 연결 코드 오류가 자동 상태 갱신에 덮이지 않는지와 연결 제출 후
초안 정리를 추가로 검사합니다. 위 다중 채팅 54회 기록은 이 팝업 개선 전의 기준선입니다.

## 이전 확장 방식: 실제 Firefox 및 Codex CLI E2E

`server/tests/firefox_e2e.rs`가 실제 Firefox를 headless 임시 프로필로 시작하고,
현재 작업 디렉터리의 확장을 임시 설치합니다. 기존 Firefox 프로필은 사용하지 않습니다.
테스트 페이지 주소는 `http://localhost:<임시 포트>`입니다. Manifest의
`127.0.0.1` 호스트 권한으로 접근할 수 없어 실제 툴바 클릭의 `activeTab`이 필요합니다.

검증한 흐름:

1. 실제 툴바 → 확장 팝업 Connect → Rust 서버 인증 연결.
2. Enable 전 snapshot 거절 → 실제 Enable 버튼 → snapshot 성공.
3. click/type/scroll 성공 및 실제 페이지 DOM/입력값/스크롤 위치 확인.
4. 사용한 snapshot 재사용 거절.
5. Disconnect → 명령 거절 → 저장 토큰으로 Connect → 허용 전 snapshot 거절.
6. 같은 문서 재허용 직후, 새 snapshot을 발급하기 **전에** 이전 snapshot의 click 거절.
   새 sessionId/grantId와 동일 documentId를 확인한 뒤 새 snapshot의 click 성공.
7. 문서 이동 및 뒤로가기 뒤 접근 거절, 수동 재허용 후 복구.
8. 본문·제목·상태 텍스트, disabled/readOnly/expanded 상태 관찰.
   입력·편집 버퍼, 숨김·clipping·화면 밖 텍스트는 제외. 같은 텍스트 노드 안의
   세로/가로 잘림도 보이는 접두부만 수집하는지 검사.
9. 25ms마다 무관한 시계 DOM이 바뀌어도 같은 snapshot으로 클릭 성공.
   줄바꿈 링크의 합집합 중심점이 링크 밖에 있어도 실제 보이는 조각으로 클릭 성공.
10. 라벨·form 목적지·주변 경고·위치·disabled/busy·상태·모달·가림막·노드 교체·
    href·입력값이 바뀌면 실행 전에 거절하고 실제 클릭 횟수/입력값은 그대로 유지.
    새 관찰로 동작 복구, 본문 12,000자 상한과 잘림 표시도 확인.
11. 기존 테스트 MCP 클라이언트를 종료하고 실제 Codex CLI가 서버를 실행.
    프롬프트에 제공하지 않은 임의 코드를 본문에서 읽고 Count를 한 번 클릭한 뒤,
    새 status 메시지를 관찰하고 그 코드를 입력하고 600px 스크롤.
    도구 결과뿐 아니라 실제 Firefox DOM/입력값/스크롤과 대조.

다중 연결 변경 전 Codex CLI `0.169.0-alpha.11.1` E2E 통과(37.24초): 버튼 `Count 1`,
입력값과 임의 관찰 코드 일치, 상태 `Count updated to 1`, 스크롤 600px.
5종 도구를 총 8회 호출했습니다. 기존 권한 lifecycle과 새 관찰 회귀를 함께 실행했습니다.
실행의 JSONL은 `/tmp/firefox-codex-mcp-observation-final/codex.jsonl`, DOM까지 확인한
검증 요약은 같은 폴더의 `verified.json`에 보관했습니다. 이 파일은 해당 로컬 실행의
임시 산출물이며 저장소에는 포함하지 않습니다.
토큰 내용은 로그/프롬프트에 기록하지 않습니다.

위 기록은 별도 MCP와 실제 Codex CLI의 E2E입니다. 실제 데스크톱 채팅을 통한 MCP
검증은 아래에서 별도로 기록합니다. 컴퓨터 사용 목록 통합은 보류한 범위이며
어느 MCP 테스트도 그 목록의 Firefox 표시를 검증하지 않습니다.

### 재현 방법

Firefox, geckodriver, Rust stable이 필요합니다. MCP 포트는 프로세스별로 자동 배정됩니다.
첫 터미널에서 **테스트 전용** 드라이버를 시작합니다.

```bash
geckodriver --host 127.0.0.1 --port 4444 --allow-system-access
```

`--allow-system-access`는 Firefox UI 자동화 권한을 줍니다. 이 테스트에서는 사용자가
임시 프로필에 한정해 명시적으로 허용했습니다. 평소 사용하는 Firefox 프로필을
드라이버에 지정하거나 외부 인터페이스에 바인딩하지 마세요.

두 번째 터미널에서:

```bash
# 실제 Firefox + Rust MCP (모델 호출/로그인 불필요)
cargo test --test firefox_e2e real_firefox_mcp --locked -- --ignored --nocapture

# 실제 Codex까지 검증 (로그인된 Codex CLI, 네트워크 및 모델 사용량 필요)
FIREFOX_E2E_CODEX="$(command -v codex)" \
FIREFOX_E2E_ARTIFACTS=/tmp/firefox-codex-mcp-observation-e2e \
cargo test --test firefox_e2e real_firefox_mcp --locked -- --ignored --nocapture
```

테스트는 임시 토큰을 생성하고 종료 시 Firefox 세션·Rust/Codex 프로세스·프로필을
정리합니다. 직접 시작한 geckodriver는 테스트 후 첫 터미널에서 Ctrl-C로 종료합니다.
WebDriver의 기본 주소는 `http://127.0.0.1:4444`이며
`FIREFOX_WEBDRIVER_URL`로 loopback 포트를 변경할 수 있습니다.

Codex는 `--ephemeral --ignore-user-config --sandbox read-only`와 실행별 MCP 설정을
사용합니다. 기존 전역 설정을 변경하지 않습니다. 상태 변경 도구는
`approval_policy="on-request"`, `approvals_reviewer="auto_review"`로 정상 승인 검토를
거칩니다. `never`에서는 상태 변경 도구가 거절되는 것도 실제로 확인했습니다.
참고: [Codex 승인 설정](https://learn.chatgpt.com/docs/config-file/config-reference).

Firefox 원격 XUL 팝업은 일반 WebDriver 프레임/BiDi 목록에 노출되지 않으므로
테스트가 Firefox 자체 Marionette actor의 UI 명령을 사용합니다. 시스템 접근 검사를
유지하며 확장 내부 grant를 직접 설정하지 않습니다. 이 테스트 경로는 Firefox 내부
API에 의존하므로 다른 버전에서는 조정이 필요할 수 있습니다.

## 이전 확장 방식: 실제 Codex 데스크톱 MCP E2E 재현

다중 연결 변경 전 기준선: 2026-11-16, Codex 데스크톱 **26.1111.21308**의 전용 테스트 채팅
`01a1e3c9-5394-74c0-9ed4-01958404f160`에서 **10단계, MCP 호출 40회**를 검증했습니다.
31회 성공과 의도한 접근 거절 9회이며, 예기치 않은 실패는 없었습니다.
호출은 status 9회 / snapshot 19회 / click 4회 / type 3회 / scroll 5회입니다.
CLI를 별도 실행한 결과가 아니라 실제 데스크톱 채팅의 MCP 호출 기록입니다.

| 단계 | 실제 확인 결과 |
| --- | --- |
| 1–2 | 도구 로드, 미연결/연결 후 미허용 탭의 읽기 거절 |
| 3 | 프롬프트에 없는 본문 코드 읽기, Count 1, 코드 입력, 상태 확인, scrollY 600 |
| 4–5 | 독립 Revoke 후 연결 유지·읽기/조작 거절, 재허용 후 old snapshot 거절·새 관찰로 복구 |
| 6–8 | Disconnect 차단, Connect만으로 권한 복원 안 됨, 명시적 재허용 후 Count 2와 입력 복구 |
| 9–10 | MCP 링크 클릭으로 새 문서 이동 → 권한 철회 → 재허용 → 새 코드 읽기·입력·Count 1 |

별도 WebDriver 관찰에서 처음 코드 `ea70715c`와 입력값이 같고 Count 1/scrollY 600임을
확인했습니다. 재연결 후에는 Count 2와 `ea70715c-reconnected`, 문서 이동 후에는 새 코드
`c8c5d79c`와 입력값 및 Count 1을 대조했습니다. 같은 문서 재연결 시 session/grant ID가
바뀌고 document ID는 유지됐으며, 이동 후 document ID가 달라졌습니다.
MCP 출력 원문에 fixture의 private buffer/숨김 텍스트 표식과 bridge token이 없음을 검사했습니다.

정리된 증거는 [desktop-e2e-2026-11-16.json](desktop-e2e-2026-11-16.json),
해당 실행의 상세 MCP 원문·채팅 기록·DOM·스크린샷은
`/tmp/firefox-codex-desktop-e2e/`에 있습니다. 하네스는 623.68초 동안 유지됐고,
별도의 실제 Firefox 전체 회귀도 5.81초에 통과했습니다. 테스트 프로필과 테스트 서버,
geckodriver를 종료한 뒤 4444/8765 포트가 닫혔음을 확인했습니다. 프로젝트 MCP 설정과
0600 토큰 파일은 다음 수동 사용을 위해 유지합니다. 개인 Firefox 프로필은 변경하지 않았습니다.

이 기준선에서 두 번째 서버가 `Address already in use`로 종료됐던 제약은
이후 [다중 채팅 설계](multi-chat-design.md)의 동적 포트와 독립 소유권으로 수정했습니다.
아래 하네스는 현재 연결 코드를 받도록 갱신했으며, 위 40회 기록은 변경 전 결과입니다.

프로젝트의 `.codex/config.toml`에 `examples/codex-extension-config.toml`의 절대경로를 설정한 뒤
이 프로젝트에서 **실제 데스크톱 테스트 채팅**을 만듭니다. 기존 설정을 덮어쓰지 않으며
토큰 파일은 Git에 포함하지 않고 0600으로 보관합니다. 새 채팅에서 firefox 도구가
발견되는지부터 확인합니다. 기존 프로필을 드라이버에 연결하지 않습니다.

위 loopback 드라이버를 실행한 다음:

```bash
FIREFOX_MCP_TOKEN_FILE=/absolute/path/to/private/bridge.token \
FIREFOX_DESKTOP_ARTIFACTS=/tmp/firefox-codex-desktop-e2e \
cargo test --test firefox_e2e desktop_firefox_fixture --locked -- --ignored --nocapture
```

이 하네스는 별도 MCP 서버나 Codex CLI를 실행하지 않습니다. 데스크톱이 서버를
시작하며, 데스크톱 모델이 5개 firefox MCP 도구를 호출합니다. 하네스는 실제 Firefox
툴바/팝업 버튼만 조작하고 페이지 결과를 독립적으로 읽습니다.

제어 파일 `command.json`에 증가하는 `seq`와 `action`을 기록합니다. 예:
`{"seq":1,"action":"connect","connectionCode":"<port>:<clientId>"}`.
`FIREFOX_DESKTOP_MULTI=1`로 시작하면 A/B 두 탭을 만듭니다. 이후 명령의 `tab`은
`A`/`B`, `clientId`는 팝업에서 선택할 연결입니다. 지원 동작은 `connect`, `enable`,
`conflict`(중복 소유 거절 확인), `revoke`, `disconnect`, `disconnectAll`, `forget`,
`inspect`, `navigate`, `finish`입니다. 완료 결과는 원자적으로 갱신되는
`result.json`과 `step-N.json`에 저장합니다. 각 단계 완료를 기다린 뒤 데스크톱 채팅에서
해당 상태의 MCP 호출을 수행합니다. `finish` 또는 30분 제한 후 임시 Firefox를 종료합니다.

각 단계의 스크린샷은 WebDriver의 원본 PNG base64인 `step-N-*.png.b64`입니다.
스크린샷에 headless Firefox의 별도 팝업이 포함되지 않을 수 있으므로 팝업 상태는
실제 팝업 DOM에서 읽은 `popupStatus`도 함께 보관합니다. 이 파일은 테스트 fixture만
포함해야 하며 실제 계정·개인 페이지에 하네스를 사용하지 않습니다.

## 이전 확장 방식의 검증 범위 한계

- 실제 검증은 위 Linux/Firefox/Codex 버전과 로컬 fixture에 한정합니다.
  Firefox 최소 지원 버전 전체, macOS, 로그인 사이트·결제 흐름은 검증하지 않았습니다.
- 뒤로가기 후 재허용은 실제 브라우저에서, BFCache lifecycle은 모의 회귀에서 검사했습니다.
- URL만 바뀌는 SPA 이동도 현재는 허용을 해제합니다. 무관한 DOM 변경은 허용하지만
  대상·주변 문맥·상태·모달·레이아웃·스크롤 변화에는 새 snapshot이 필요합니다.
  긴 문맥, 지속적인 live-status 갱신, 움직이는 대상은 보수적으로 거절할 수 있습니다.
- DOM 관찰은 화면의 모든 픽셀이나 접근성 트리를 재현하지 않습니다. iframe/shadow DOM,
  CSS 생성 텍스트, canvas는 없으며 fixed/overflow 특수 레이아웃은 누락될 수 있습니다.
- 이미 페이지로 전달한 동작은 철회로 취소할 수 없습니다. 응답 유실 시 unknown으로
  취급하고 자동 재시도하지 않습니다.
- CI는 fmt/clippy/Rust/JS 회귀를 실행합니다. 실브라우저 및 로그인된 Codex E2E는 opt-in입니다.
  이번 변경의 원격 CI는 아직 실행하지 않았습니다.
