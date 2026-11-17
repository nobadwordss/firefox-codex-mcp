# Windows 10 / 11 지원 계약

상태: 0.5.0-beta.1에 Windows 어댑터와 설치기를 구현했습니다. 아래는 정식 지원을 위한 검증 기준입니다.
[프리뷰 구현과 현재 검증 범위](windows-preview.md)를 확인하세요. 0.4.x 실행 파일은 Linux 전용입니다.
첫 대상은 Windows 10 22H2 / Windows 11의 x86_64, 일반 Win32 Firefox와 로컬 Codex다.
각 OS에서 실제 설치와 데스크톱 E2E를 통과해야 지원 목록에 추가한다.
Store/MSIX Firefox와 ARM64는 별도 검증 전까지 포함하지 않는다.

## 플랫폼과 무관하게 유지할 부분

- 현재 Mozilla 서명 확장의 ID, 자동 연결, 무작위 프로필 식별자, 사용자 프로필 선택.
- 채팅별 새 탭 소유권, 기존 탭 인수 금지, pause/revoke/EOF/재연결 시 권한 폐기.
- MCP 도구 이름·스키마와 콘텐츠 스냅샷 검증. 플랫폼에 따라 도구를 다시 학습하지 않는다.
- Native Messaging의 4바이트 길이 + JSON 프레임과 내부 라우터의 버전 있는 NDJSON.
  OS 이식과 프로토콜 형식 변경을 동시에 하지 않는다.
- 페이지 코드·토큰·프로필 경로를 입력하지 않는 설치 흐름.

## 먼저 분리할 Rust 경계

| 경계 | Linux 현재 구현 | Windows 구현 요구 |
|---|---|---|
| 로컬 IPC | private Unix socket, UID 확인 | Named Pipe, 명시적 DACL과 로컬 사용자·로그온 세션 확인 |
| 실행 중복 방지 | flock | 첫 파이프 인스턴스/사용자 범위 잠금, 이름 선점 시 안전하게 실패 |
| 설치 경로 | HOME 아래 private 디렉터리 | Known Folder API의 LocalAppData, Unicode 경로 지원 |
| Native host 등록 | 사용자 native manifest + 셸 래퍼 | HKCU NativeMessagingHosts + native `.exe` 진입점 |
| OS 난수 | `/dev/urandom` | OS CSPRNG를 제공하는 공통 Rust 라이브러리 |
| 파일 교체·복구 | 원자 rename, fsync, 장부 | 버전별 설치 디렉터리, 등록 경로 전환과 실패 복구 |
| Firefox 실행 | 일반 Firefox 실행 파일 | App Paths/설치 정보 기반 탐색, 문자열 셸 실행 금지 |
| 프로세스 종료 | Unix signal/EOF | EOF와 Windows 종료 알림; Firefox 프로세스는 종료하지 않음 |

`native.rs`의 프레임 처리는 이미 AsyncRead/AsyncWrite를 사용한다. 연결 생성·peer 검사·
설정 파일·프로세스 시작을 플랫폼 모듈로 옮기고, 라우터는 동일한 인증된 스트림을 받게 한다.
`debug.rs`, `setup.rs`, `lifecycle.rs`, `onboarding.rs`, `manager.rs`, 패키지 도구에 남은
Unix 전용 import, libc, `/proc`, 실행 파일 경로도 분리해야 한다. BiDi는 고급 옵션이며
Windows 기본 확장 연결의 선행 조건으로 만들지 않는다.

## 연결 주소와 권한

실제 Linux 전환에서 Firefox는 XDG_RUNTIME_DIR를 상속하고 Codex MCP는 생략해
라우터가 둘로 갈라졌다. 0.3.1 확장 모드는 HOME 기준의 고정 private 주소를 사용한다.
Windows도 프로세스별 TEMP나 작업 디렉터리에 의존하지 않고 동일 사용자·로그온 세션의
두 클라이언트가 같은 endpoint를 계산해야 한다. 테스트 override는 양쪽을 명시적으로
격리할 때만 사용한다.

Linux의 HOME 경로가 매우 길면 Unix socket 주소 길이 제한에 도달할 수 있다.
0.3.1은 이 제한을 우회하는 별도 주소 선택을 구현하지 않는다. 후속 경로 추상화에서
길이 검증과 양쪽 프로세스가 동일하게 계산하는 대체 주소를 함께 다룬다.

Named Pipe의 기본 보안 설명자는 Everyone/익명 읽기를 허용할 수 있으므로 그대로 쓰지
않는다. 명시적인 DACL, 원격 클라이언트 거부, 서버/클라이언트 신원 확인, 파이프 이름 선점
거부를 포함한다. 같은 OS 사용자로 실행되는 악성 프로그램에 대한 샌드박스를 주장하지 않는다.
[Microsoft의 파이프 보안 문서](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights).
Rust의 비동기 전송은 [Tokio Named Pipe](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/index.html)를 기반으로 구현할 수 있다.

사용자 범위의 `HKCU\Software\Mozilla\NativeMessagingHosts\firefox_codex_mcp`에
manifest 위치를 등록한다. manifest의 allowed_extensions는 현재 서명 ID를 유지한다.
일반 설치에 관리자 권한이나 시스템 전체 레지스트리 수정을 요구하지 않는다.
[Mozilla Native Messaging 설치 규칙](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging#windows_setup).

## 설치·업데이트·제거

서명된 설치 프로그램에서 helper와 전역 MCP 등록을 준비하고 Firefox의 표준 확장 설치
확인으로 이어간다. Native host는 Windows에서 인수를 넘길 수 있는 고정 `.exe` 진입점이
필요하므로, manifest 경로의 인수처럼 보이는 문자열에 의존하지 않는다. Authenticode 서명과
SmartScreen 동작도 실제 다운로드 환경에서 확인한다.

실행 중인 `.exe`의 덮어쓰기나 rename이 항상 성공한다고 가정하지 않는다. 버전별 경로에
새 바이너리를 설치하고 manifest/MCP 등록을 원자적으로 전환한다. 옛 연결은 권한을 해제한
뒤 종료시키며, 실행 파일 정리는 사용이 끝난 다음 수행한다. 실패하면 이전 등록으로 복구한다.
제거는 장부에 기록한 파일과 레지스트리 값만 비교 후 지우며 사용자 수정은 보존한다.

## 출시 전 시험

1. Windows 10과 11의 깨끗한 일반 사용자 계정에서 설치·업데이트·제거·복구.
2. 실제 Firefox 서명 확장 설치, 일반 실행, 재시작, 두 프로필과 복제 프로필 처리.
3. 프로젝트 안/밖의 여러 새 Codex 데스크톱 채팅에서 자율 도구 선택과 실제 조작.
4. 다른 채팅의 탭 거부, 철회 후 읽기 거부, 한 채팅의 철회가 다른 채팅에 영향 없음.
5. Firefox와 Codex의 서로 다른 환경 변수, 공백·한글 사용자 경로, 동시 실행·업데이트.
6. 다른 Windows 사용자/원격 클라이언트의 파이프 접근 및 파이프 선점 거부.
7. Windows runner의 빌드·unit test만으로 완료 처리하지 않고 실제 Firefox + Codex VM 증거 보존.
