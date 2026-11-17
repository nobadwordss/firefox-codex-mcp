# 디버깅 기반 AI 작업 탭

> Historical BiDi design record from 2026-11-16, not current installation guidance.
> BiDi is an advanced Linux-only compatibility option; the launcher behavior below
> describes earlier builds. The current source creates no app-menu launchers.
> Use the default [Firefox extension connection](extension-mode.md) and its
> [Firefox Settings flow](firefox-only-management.md) for ordinary use.

사용자가 2026-11-16 명확히 한 모델은 AI가 현재 Firefox 프로필에 작업 탭을 직접 열어
사용하는 것입니다. 사용자가 탭을 선택해 매번 허용하는 모델을 기본 흐름에서 제외합니다.
Native Messaging을 이용한 수동 허용 단순화 시안은 배포하지 않고 이 설계로 전환했습니다.

## 연결과 소유권

- Rust 바이너리 `--bidi`가 MCP stdio를 제공하고, `--bidi-broker`가 Firefox의 단일 BiDi 세션을 소유합니다.
- broker와 Firefox 실행은 각각 독립 프로세스 그룹을 사용합니다. 첫 채팅이나 실행기 종료가
  다른 채팅의 연결 또는 사용 중인 Firefox를 함께 종료하지 않도록 실제 프로세스 그룹 종료로 검사했습니다.
- 사용자 전용 디렉터리(0700), Unix socket(0600), 동일 UID 확인, OS 파일 잠금으로 broker 수명을 관리합니다.
  PID는 broker 인증이나 소유권 판단에 쓰지 않습니다.
- 프로필 경로를 기준으로 socket 주소를 정하므로 Firefox의 디버깅 포트가 바뀌어도 설정값 복사가 필요하지 않습니다.
  실제 Firefox 160이 생성하는 파일은 `WebDriverBiDiServer.json`이며 `ws_host`와 `ws_port`를 읽습니다.
- 각 socket 연결을 broker가 새 무작위 ID로 식별합니다. 도구 호출자가 다른 채팅 ID를 지정할 수 없습니다.
- `browsingContext.create` 결과만 최초 소유권으로 등록합니다. 기본 userContext를 사용해 프로필의 저장된 세션을 공유합니다.
  새 탭은 `background=true`로 열어 사용자가 보고 있는 탭의 포커스를 빼앗지 않습니다.
- `contextCreated`에서 `parent=null`, `originalOpener`가 소유 탭인 경우에만 자식 탭을 배정합니다.
  iframe이나 unrelated user tab은 가져오지 않습니다. 선택도 소유권 검사 후 수행합니다.
  iframe 안에서 열린 팝업의 opener 조상 추적은 현재 지원하지 않습니다.
- 페이지 이동은 소유권을 유지하지만 DOM 스냅샷은 새로 받아야 합니다. 새 문서·BFCache·오래된 참조를 방어합니다.
- `firefox_revoke`, stdio 종료, 디버깅 연결 손실은 소유권을 폐기합니다. 열린 결과는 남기며 재연결 시 인수하지 않습니다.
  명시적 `firefox_close_tab`과 사용자의 Firefox 탭 닫기는 실제 탭을 닫습니다.
- BiDi session을 새로 만들면 남아 있는 물리 탭의 context ID도 바뀔 수 있습니다. 세션 밖에서 ID를 저장해 재사용하지 않습니다.

## 관찰과 조작

`extension/page.js`가 기존 관찰·동작 검증을 전송 방식에서 분리한 공통 엔진입니다.
기존 확장과 BiDi 모두 같은 엔진을 사용합니다. BiDi는 별도 sandbox realm에서 실행하고
JSON 문자열로 결과를 돌려받습니다. 일반 페이지가 제어 상태를 바꾸거나 반환 핸들이 누적되는 것을 피합니다.
MCP에 임의 JavaScript, 원시 BiDi, cookie API나 전체 탭 목록 도구를 노출하지 않습니다.

## 설치와 실제 제약

`--install-debug PROFILE`은 사용자 전용 위치에 바이너리를 설치하고 Linux 실행기를 만듭니다.
`remote.prefs.recommended=false`로 자동화 권장 설정 적용을 건너뛰며 기존 user.js를 백업합니다.
`--launch-firefox PROFILE`은 같은 프로필로 동적 디버깅 포트를 켭니다. 실행 중인 Firefox를 강제 종료하지 않습니다.
Firefox가 꺼져 있으면 첫 `firefox_open_tab`이 실행을 맡습니다. 일반 모드로 이미 켜져 있으면 최초 재시작이 필요합니다.
실행 전 프로필의 기존 디버깅 포트 파일을 제거해 비정상 종료로 남은 포트를 재사용하지 않습니다.

디버깅 자체는 해당 프로필에 넓은 접근이 가능한 기능입니다. 채팅별 탭 제한은 broker의
논리적 경계이며 OS나 Firefox 자체의 보안 격리로 과장하지 않습니다. 시스템 접근 옵션은 사용하지 않습니다.

## Gemini 제안에 대한 독립 판단

단일 broker, sandbox realm, 순수 JSON 반환, opener 기반 자식 탭 검토는 반영했습니다.
프로세스 종료 때 탭을 자동으로 닫자는 제안은 결과/입력 보존을 위해 반영하지 않았습니다.
`DevToolsActivePort`를 읽으라는 제안도 실제 Firefox 생성 파일을 확인해 교정했습니다.
Mozilla 공식 MCP 그대로 사용하거나 앞에 wrapper를 붙이는 방식은 단일 세션·활성 탭 선택과
우리의 채팅별 소유권을 별도로 조정해야 합니다. 기존 Rust/관찰 엔진을 재사용하는 직접 BiDi 방식을 선택했습니다.

근거: [MDN BiDi 연결](https://developer.mozilla.org/en-US/docs/Web/WebDriver/How_to/Create_BiDi_connection),
[단일 세션 제한](https://developer.mozilla.org/en-US/docs/Web/WebDriver/Reference/BiDi/Modules/session/new),
[Firefox Remote Agent 보안 모델](https://firefox-source-docs.mozilla.org/remote/Security.html),
[자동화 설정](https://firefox-source-docs.mozilla.org/remote/Prefs.html),
[Mozilla 공식 MCP](https://github.com/mozilla/firefox-devtools-mcp).
