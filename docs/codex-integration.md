# Codex 데스크톱의 Firefox 통합 경계

아래 Foxhand/확장 비교는 수동 탭 허용 방식을 검토하던 시점의 기록입니다.
현재 기본 구현은 [디버깅 기반 AI 작업 탭](easy-connection-design.md)으로 전환했습니다.
컴퓨터 사용 목록 통합을 보류한 결정은 유지합니다.

2026-11-16에 사용자 화면과 설치된 앱을 확인한 뒤, 사용자와 합의해 **컴퓨터 사용 목록
통합은 보류하고 공식 MCP 설정을 통한 Firefox 연결·관찰·조작을 우선**합니다.
별도 MCP 등록은 해당 설정 화면에 브라우저 행이나 네이티브 탭 멘션을 추가하지 않습니다.
아래 앱 분석은 보류 결정의 근거이며, 설치 앱 바이너리는 변경하지 않았습니다.

## 설치 앱에서 확인한 등록 경로

대상은 Linux `/usr/lib/chatgpt`, 패키지 `openai-codex-electron`
**26.1111.21308**입니다. 앱과 사용자 프로필은 변경하지 않았습니다.
진단 JSON에 검사한 번들의 SHA-256을 기록했습니다.

1. **고정 브라우저 카탈로그:** `app.asar`의 `.vite/build/early-bootstrap.js`
   모듈 `79895`는 chrome/edge/brave/opera/vivaldi만 정의합니다.
   실제 family 검사 함수를 실행하면 `firefox`, `zen`은 false입니다.
   생산 채널의 확장 ID 집계도 Chromium ID 형식(`[a-p]` 32자)을 검사합니다.
   `firefox-codex-mcp@local.invalid`를 추가 인자로 주면 예외가 납니다.
2. **설정 화면:** `webview/assets/async/28918.48c9cffd9d.js`는
   `Object.keys(c.Ypn).filter(c.rUV)`로 카탈로그를 순회하고 설치된 family와
   조합해 행을 만듭니다. 연결된 임의 공급자의 표시 이름을 새 행으로 합치지 않습니다.
   브라우저별 아이콘·참조 표시 토글도 별도 매핑을 사용합니다.
3. **설치/확장 감지:** main 번들의 `getInstalledBrowserFamilies()` 역시 카탈로그를
   순회합니다. `getExtensionStatus()`는 family를 먼저 검증하고 Chromium 프로필의
   확장 설치를 검사합니다. Firefox의 `extensions.json`을 읽는 경로는 확인되지 않았습니다.
4. **네이티브 연결 발견:** main 번들의 `MR` 함수는 현재 UID가 소유한 `.sock`에
   연결해 `getInfo`를 요청합니다. `type`, 탭 멘션 프로토콜 버전, family,
   build channel, extension ID, instance ID, generation을 검증합니다.
   생산 앱은 허용된 생산 확장 ID만 받아들입니다. family를 `firefox`로 보낸다고
   등록되지 않으며, 거절된 연결은 닫습니다.
5. **실제 제어:** 번들 `browser-service.mjs`에는
   `Page.captureScreenshot`, `Accessibility.getFullAXTree`, `DOM.getFrameOwner`,
   `Input.dispatchMouseEvent`, debugger target 연결을 사용하는 경로가 있습니다.
   이 저장소의 관찰·클릭·입력 MCP 도구와는 다른 계약입니다.

따라서 `extension-ids.json`이나 MCP 설정만 편집하는 방식은 이 빌드의 네이티브
Firefox 등록 경로가 아닙니다. 별도 native host 매니페스트만 설치해도 앱의
family/ID 검사와 설정 카탈로그는 그대로입니다. 공식 안내의 지원 목록도
[Chrome/Edge/Brave/Opera/Vivaldi](https://learn.chatgpt.com/docs/chrome-extension)입니다.
미래 빌드나 앱 코드 자체를 변경한 경우까지 불가능하다는 뜻은 아닙니다.

## 재현 가능한 검사

```sh
node tools/audit-codex-browser-integration.cjs /usr/lib/chatgpt
```

[검사 도구](../tools/audit-codex-browser-integration.cjs)는 **읽기 전용**입니다.
설치된 JavaScript 함수를 그대로 재현하기 위해 Node를 사용합니다.
버전과 네 개 번들 및 browser-service의 해시를 확인한 뒤 앱 시작점을 제거하고,
순수 카탈로그/상수 함수와 실제 발견 함수를 실행합니다. 발견 함수의 파일·소켓 I/O는
메모리 fixture로 대체합니다. 실제 앱 소켓이나 사용자 브라우저에는 연결하지 않습니다.
Node `vm`을 임의 코드의 보안 샌드박스로 취급하지 않으며, 검토한 해시만 실행합니다.
새 앱 빌드는 재검토하기 전까지 실패합니다.

[2026-11-16 검사 결과](codex-native-audit-26.1111.21308.json):

| 발견 함수에 준 합성 메타데이터 | 결과 |
| --- | --- |
| Chrome + 허용된 생산 ID + 생산 채널 | 수락 — 양성 대조군 |
| Firefox + 이 저장소의 Gecko ID | 거절 및 연결 닫기 |
| Firefox + 허용된 생산 ID | 거절 및 연결 닫기 |
| Chrome + Gecko ID | 거절 및 연결 닫기 |
| Chrome + 허용된 ID + 잘못된 채널 | 거절 및 연결 닫기 |

추가로 손상된 ASAR, 다른 버전, 해시가 달라진 번들의 실행 전 거절과 `--help`를
검사했습니다. **이 검사는 앱 UI나 실제 Firefox E2E가 아닙니다.**
공식 ID를 쓰는 대조군도 메모리 안에서만 실행했으며 실제 연결의 신원을 바꾸지 않았습니다.

## 공개 소스와 후보 프로젝트

[openai/codex](https://github.com/openai/codex)는 CLI와 Rust app-server 소스를
제공합니다. 데스크톱 설정 UI의 공식 소스 저장소는 이번 검색에서 찾지 못했습니다.
OpenAI 기여자의 [2026-02-05 답변](https://github.com/openai/codex/issues/10733#issuecomment-3854408866)은
당시 앱 소스 공개 계획이 없으며 앱이 공개 app-server 위에 구현됐다고 설명합니다.
이는 현재 시점의 영구 비공개 보장이 아니라 확인 가능한 공개 답변입니다.
설치된 ASAR의 20,220개 파일에는 소스맵과 `node_modules` 밖의 TypeScript/Rust
원본이 없었습니다. 축소된 JavaScript 번들은 읽을 수 있습니다.

| 프로젝트 | 확인한 연결 방식 | 목표 화면과의 관계 |
| --- | --- | --- |
| [Browsentic](https://github.com/imshaikot/browsentic) | 자체 Firefox 확장/Bridge, MCP 및 Codex app-server `dynamicTools` | 자체 사이드패널/에이전트 연결. 데스크톱 family 등록 경로는 아님 |
| [Foxhand](https://github.com/hamb1y/foxhand) | Firefox Native Messaging helper가 localhost MCP HTTP 제공 | Codex `mcp_servers.firefox` 설정. 컴퓨터 사용 행 추가는 아님 |
| [Codex Computer Use for Firefox/Zen](https://github.com/SunkenInTime/codex-computer-use-firefox-zen) | Firefox 호환 계층과 공식 extension-host 사이의 relay | 소스에서 ID/Origin을 공식 Chrome 값으로 변환. 독립 Firefox 등록 성공의 근거가 아님 |
| [codex-unpacked-toolkit](https://github.com/trojanSF/codex-unpacked-toolkit) | 배포된 앱을 추출·패치하는 제3자 도구 | 원본 데스크톱 소스나 Firefox 백엔드는 아님 |

사용자가 제공한 [Reddit 글](https://www.reddit.com/r/codex/comments/1xhssqz/gave_codex_a_way_to_drive_my_real_firefox_instead/)은
Foxhand 소개입니다. 기술적 판단은 연결된 저장소의 README와 구현을 함께 확인했습니다.
후보 소프트웨어는 설치하거나 실행하지 않았습니다.

### Browsentic에서 직접 확인한 부분

검사 커밋: `9c0892a457e86cf2a5c3d1f4a8eea7eeb0900632`.
[Codex runner](https://github.com/imshaikot/browsentic/blob/9c0892a457e86cf2a5c3d1f4a8eea7eeb0900632/src/daemon/agent/runners/codex-app-server.ts)는
`thread/start.dynamicTools`를 전달하고 `item/tool/call`을 처리합니다.
이는 실행 중인 데스크톱 앱의 브라우저 카탈로그를 등록하는 API가 아닙니다.
[MCP 안내](https://github.com/imshaikot/browsentic/blob/9c0892a457e86cf2a5c3d1f4a8eea7eeb0900632/docs/guide/mcp-clients.md)도
Codex CLI의 `config.toml` 등록을 설명합니다.
[Firefox 제한](https://github.com/imshaikot/browsentic/blob/9c0892a457e86cf2a5c3d1f4a8eea7eeb0900632/docs/guide/limits.md#nine-tools-need-chromes-debugger-and-firefox-has-none)은
trusted click, diagnostics 등 CDP 도구 9개를 Firefox 목록에서 제외한다고 명시합니다.
사용자 승인·도구 지원 범위 표시·본문 읽기·스크린샷 구현은 이 저장소 개선의 참고가 됩니다.

### Foxhand에서 직접 확인한 부분

검사 커밋: `7dcb176f4a2c95c3b5fef99df66027d27aa362be`.
[host.ts](https://github.com/hamb1y/foxhand/blob/7dcb176f4a2c95c3b5fef99df66027d27aa362be/mcp-server/src/host.ts)는
Firefox Native Messaging 입출력과 MCP HTTP 서버의 생명주기를 연결합니다.
[http.ts](https://github.com/hamb1y/foxhand/blob/7dcb176f4a2c95c3b5fef99df66027d27aa362be/mcp-server/src/http.ts)는
MCP SDK의 `McpServer`/`StreamableHTTPServerTransport`와 bearer 인증을 사용합니다.
[README](https://github.com/hamb1y/foxhand/blob/7dcb176f4a2c95c3b5fef99df66027d27aa362be/README.md)의
Codex 연결은 `mcp_servers.firefox`에 `/mcp` URL을 추가하는 방식입니다.
네이티브 메시징이라는 이름이 같아도 Codex extension-host의 발견/제어 프로토콜과 같지는 않습니다.

### MCP 방향에서 활용한 점과 직접 만드는 이유

Foxhand의 [background.js](https://github.com/hamb1y/foxhand/blob/7dcb176f4a2c95c3b5fef99df66027d27aa362be/extension/background.js)는
사용자가 일시 중지했을 때 명령을 거절하고 재개 방법을 설명합니다. 연결 상태 진단,
중지 UI와 복구 안내라는 설계를 참고해 이 프로젝트에 독립 **Revoke tab access**와
`connected`/`tabEnabled`/`nextStep`을 구현했습니다. 코드를 복사하거나 Foxhand를
의존성으로 추가하지 않았습니다. Foxhand는 MPL-2.0이며 추후 코드를 직접 재사용한다면
해당 라이선스 조건을 별도로 따라야 합니다.

| 관점 | Foxhand (검사 커밋) | 이 프로젝트 |
| --- | --- | --- |
| 연결 | Native Messaging helper가 Firefox와 함께 MCP HTTP 제공 | Codex가 Rust stdio MCP 시작, 확장에서 Connect |
| 범위 | README 기준 45개 도구, 탭·다운로드·북마크 등 폭넓은 기능 | 상태·관찰·클릭·입력·스크롤 5개 도구 |
| 확장 권한 | `<all_urls>`, cookies/history/downloads 등을 요청 | activeTab으로 사용자가 허용한 탭, cookie/history API 없음 |
| 중단·복구 | pause/resume와 연결 상태 안내 | 연결 유지형 철회, 문서/연결 변경 후 명시적 재허용 |
| 주력 가치 | 기능 범위와 자동 기동 편의 | 좁은 권한 범위, 오래된 대상의 동작 차단, 검증 가능한 철회 경계 |

권한 범위 근거는 Foxhand의
[manifest.json](https://github.com/hamb1y/foxhand/blob/7dcb176f4a2c95c3b5fef99df66027d27aa362be/extension/manifest.json)과
이 저장소의 manifest입니다. 이 비교는 Foxhand의 보안 결함이나 실제 성능 열세를
뜻하지 않습니다. Foxhand의 런타임 E2E나 모든 snapshot 방어를 비교 검증하지 않았습니다.

일반적인 다중 페이지 자동화가 목적이라면 Foxhand를 쓰는 편이 경제적일 수 있습니다.
직접 만드는 가치는 **권한 범위를 작게 유지하면서 철회·재허용·오래된 DOM 대상을
실제 데스크톱 E2E로 입증하는 도구**에 있습니다. Rust라는 언어 자체만으로 더 안전하다고
주장하지 않습니다. 수동 재허용, 연결 코드 입력, 임시 확장 설치의 마찰을 줄이지 못한다면
유지보수 비용이 차별화 가치를 넘을 수 있습니다.

Gemini와 재검토한 결론도 기능 수 경쟁보다 연결/권한 상태와 복구 흐름을 먼저 완성하자는
것입니다. [다중 채팅 격리](multi-chat-design.md)는 이후 구현했으며, 서명된 배포와 native host 자동 기동은
도입 가치와 권한 소유권을 함께 검토합니다.

### 패치 가능성과 아직 검증하지 않은 부분

앱 번들을 직접 패치하는 경로는 기술적으로 배제하지 않았지만, 단일 브라우저 이름이나
아이콘 변경으로는 완료되지 않습니다. 현재 바이너리의 Electron fuse는
`EnableEmbeddedAsarIntegrityValidation`과 `OnlyLoadAppFromAsar`가 켜져 있습니다.
다만 [Electron 문서](https://www.electronjs.org/docs/latest/tutorial/asar-integrity)는
무결성 검사 지원 OS로 macOS/Windows를 명시하므로, 이 값만으로 Linux 패치 기동이
차단된다고 결론 내리지 않았습니다. 앱 복사본의 패치·기동은 실행하지 않았고,
무결성 검사나 확장 신원 검사를 끄지도 않았습니다.

Gemini와 카탈로그·발견 함수·CDP 의존성 및 위 두 후보의 연결 경로를 교차 검토했습니다.
Gemini의 답변은 검토 의견이며, 위 결과의 근거는 설치 코드 실행과 링크한 원본 구현입니다.

## 앱 측에 필요한 작업

다음은 확인한 구조에서 도출한 통합 요구사항입니다. 공개되지 않은 API가 이미
존재한다고 가정하지 않습니다.

1. main/renderer의 카탈로그, 설치 감지, 아이콘, 브라우저 참조 토글에 Firefox를 추가합니다.
2. Gecko ID와 Firefox Native Messaging 매니페스트를 위한 명시적인 등록 계약을
   추가합니다. 기존 공식 Chrome ID로 신원을 덮어쓰는 방식은 사용하지 않습니다.
3. native discovery와 browser-service가 Firefox 공급자를 선택하도록 연결하고,
   CDP 의존 기능을 Firefox용 구현으로 분기합니다. 구현되지 않은 기능은 명시적으로
   지원하지 않는다고 보고해야 합니다. 현재의 MCP 도구를 같은 계약이라고 가정하지 않습니다.
4. 앱의 사이트 권한·탭 허용·철회·연결 해제와 Firefox 동작을 연결하고, 배포 가능한
   확장 설치와 앱 재시작 후 복구까지 아래 완료 조건으로 검사합니다.

현재 권한·관찰 엔진과 E2E fixture는 이 작업의 검증 기반으로 재사용할 수 있습니다.
진행에는 앱 수정 경로 또는 공식 브라우저 공급자 등록 계약이 필요합니다.

## 완료 판정

| 확인 항목 | 현재 결과 |
| --- | --- |
| Codex CLI → MCP → 실제 Firefox 관찰·클릭·입력·스크롤 | 통과; 임시 프로필, 로컬 fixture |
| 실제 Codex 데스크톱 채팅 → MCP → Firefox 읽기·클릭·입력·스크롤 | 통과; 26.1111.21308, 임시 프로필 |
| 데스크톱에서 독립 철회·재허용·재연결 후 접근 차단/복구 | 통과; [검증 기록](validation.md) |
| 철회·재허용·이동 후 이전 스냅샷 거절 | 통과 |
| 본문·상태 읽기 및 동적 DOM 대상 검증 | 통과; [검증 기록](validation.md) |
| 데스크톱 설정에 Firefox 표시 및 설치/연결 상태 갱신 | 보류 |
| 데스크톱에서 Firefox 선택 후 네이티브 브라우저 도구로 제어 | 보류 |
| 앱 재시작·일반 프로필·서명된 확장 배포 | 미검증/미구현 |

MCP E2E 성공을 마지막 세 항목의 성공으로 대체하지 않습니다.
