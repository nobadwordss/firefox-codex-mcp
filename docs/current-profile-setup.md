# 이전 확장 방식의 현재 프로필 설정 기록

이 문서는 수동 연결 방식의 과거 절차입니다. 현재 기본 방식은 AI가 디버깅 연결로 작업
탭을 직접 생성하는 [BiDi 설정](../README.md#사용-흐름)이며, 아래 확장·코드·토큰·허용
절차가 필요하지 않습니다.

실제 데스크톱 E2E 통과와 개인 프로필의 설치 완료는 별개입니다. 당시 배포는
개발용 확장이었으므로 일반 Firefox에 확장을 등록하고, 채팅과 탭을 직접 연결해야 합니다.
임시 **프로필**을 만드는 과정이 아닙니다. 사용 중인 프로필에 임시 **확장**을 등록합니다.
확장은 Firefox를 재시작하면 다시 등록해야 하며 서명된 상시 설치 패키지는 아직 없습니다.

## 준비 상태 확인

1. 프로젝트의 `.codex/config.toml`에 서버 실행 파일과 `FIREFOX_MCP_TOKEN_FILE`의
   절대경로를 설정합니다. 토큰은 64자리 hex, 파일 권한은 0600으로 보관합니다.
   `codex mcp list`에서 `firefox`가 enabled인지 확인합니다.
2. 이 프로젝트의 새 데스크톱 채팅에서 `firefox_status`를 호출합니다.
   설정 파일에 항목이 있어도 기존 채팅에 도구가 노출됐다고 가정하지 않습니다.
3. `connected: false`는 Firefox가 아직 연결되지 않은 상태입니다. 도구 결과에 나온
   공개 `connectionCode`를 복사합니다. 비밀 토큰과 혼동하지 마세요.

## 실제 프로필에 연결

1. 평소 사용하는 Firefox에서 `about:debugging#/runtime/this-firefox`를 엽니다.
   **임시 부가 기능 로드**에서 저장소의 `extension/manifest.json`을 선택합니다.
   이미 개발용 확장을 등록했다면 Reload를 사용합니다.
2. 아래 준비한 localhost 테스트 페이지를 같은 Firefox의 새 탭에서 엽니다.
3. Firefox Codex MCP 툴바 버튼에서 해당 채팅의 연결 코드와 로컬 토큰 파일의 값을
   입력하고 **Connect**를 누릅니다. 비밀 토큰을 채팅에 붙이지 않습니다.
   연결 코드는 팝업을 닫아도 확장이 실행 중인 동안 유지되므로, 코드를 먼저 넣고
   토큰 파일을 복사하러 이동해도 됩니다. 제출하지 않은 토큰 입력은 보관하지 않습니다.
4. 연결 코드가 해당 채팅과 일치하는지 확인하고 테스트 탭에서
   **Enable current tab for selected chat**를 누릅니다.
5. 해당 Codex 채팅에서 status → snapshot → Count 버튼 클릭 → 새 snapshot →
   Message 입력 순서로 동작을 확인합니다. 테스트 뒤 Revoke하여 읽기·조작 거절을
   확인하고, 다시 Enable한 뒤 새 snapshot으로 복구를 확인합니다.

권한 부여는 선택한 채팅과 탭에 한정됩니다. 기존 로그인 탭이나 Firefox 프로필 전체를
허용하는 버튼이 아닙니다. 새 문서로 이동하거나 연결이 끊기면 다시 허용해야 합니다.

## 기존 프로필용 테스트 페이지

기존 Rust E2E fixture를 브라우저 실행 없이 사용할 수 있습니다. 실행마다 새로운
산출물 디렉터리를 지정합니다.

```bash
FIREFOX_PROFILE_ARTIFACTS=/tmp/firefox-profile-check-1 \
cargo test --test firefox_e2e current_profile_fixture --locked -- --ignored --nocapture
```

출력 또는 `ready.json`에 있는 `http://localhost:<port>`를 사용합니다. 서버는 loopback에만
바인딩하며 Firefox 실행·프로필 변경·확장 설치·권한 부여를 하지 않습니다.
테스트 코드의 전체 내용을 검토할 수 있고 본문 코드는 페이지가 새로 만들어줍니다.
산출물 디렉터리에 `finish.json` 파일을 만들거나 30분이 지나면 페이지 서버가 종료됩니다.

## 아직 남은 제품화

- 서명된 확장과 재시작 후 유지되는 설치·업데이트 경로.
- 연결 코드 입력, 상태 확인, 오래된 연결 정리를 쉽게 하는 초기 설정 안내.
- 실제 사이트 및 다른 지원 OS/Firefox 버전 검증.

현재 검증 범위와 제한은 [검증 기록](validation.md), 채팅별 소유권과 연결 수명주기는
[다중 채팅 설계](multi-chat-design.md)에 있습니다. 네이티브 컴퓨터 사용 목록 통합은
보류 상태이며, MCP 설정만으로 목록에 Firefox 행이 생기지는 않습니다.
