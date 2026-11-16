# 기본 확장 연결 (0.3)

Native Messaging은 Firefox가 확장 전용 로컬 프로세스를 실행하는 연결 방식입니다.
Codex와 Firefox는 각각 stdio를 소유하므로 Rust 라우터가 private Unix socket으로
두 연결을 중계합니다. 설치기는 사용자 native manifest와 공식 Codex MCP 등록을 준비합니다.
확장에는 token/port/code/profile path 입력이 없습니다.

## 구성과 경계

- `--extension`: MCP stdio. 채팅마다 라우터에서 무작위 owner ID를 발급합니다.
- `--native-host`: Firefox length-prefixed JSON과 라우터 newline JSON을 변환합니다.
- `--extension-broker`: 동일 UID, 0700 디렉터리와 0600 Unix socket, 단일 프로세스 잠금.
- 0.3.1부터 기본 socket은 HOME 아래 `.local/state/firefox-codex-mcp/runtime`에 둔다.
  Firefox와 Codex가 XDG_RUNTIME_DIR를 다르게 상속해도 주소가 일치한다.
  명시적인 FIREFOX_MCP_RUNTIME_DIR는 격리 시험용으로만 양쪽에 동일 지정한다.
- 확장은 storage.local의 무작위 profileId로 현재 프로필을 등록합니다.
- 첫 작업에서 유일한 프로필을 기본으로 저장합니다. 여러 프로필은 확장의 선택 버튼을
  요구합니다. 기본 프로필이 오프라인이어도 다른 프로필을 사용하지 않습니다.
- 복제한 동일 profileId가 동시에 접속하면 양쪽을 차단합니다. 확장에서 복사된 프로필
  연결 초기화 후 원하는 프로필을 선택합니다. 순차 실행되는 복제본을 완전히 식별하지는 못합니다.
- Native Messaging ID와 UID 검사는 동일 OS 사용자 내부의 암호학적 애플리케이션 인증이
  아닙니다. 로컬 악성 프로세스에 대한 격리를 주장하지 않습니다.

## 탭 수명

확장은 owner별로 자신이 생성한 탭만 저장합니다. 사용자 탭이나 페이지 팝업을 인수하지 않습니다.
owner를 명령 대기열에 넣기 전에 등록하고, revoke/EOF/pause 시 동일 owner 객체를 무효화합니다.
따라서 탭 생성이 늦게 완료되어도 취소된 owner에 배정하지 않습니다. 결과 탭은 남겨 둡니다.

페이지 이동은 탭 소유권을 유지하고 콘텐츠 포트를 닫습니다. 다음 snapshot에서만 재바인딩합니다.
콘텐츠 세계의 documentId와 grant/session/snapshot을 검사한 뒤 비동기 양보 없이 조작합니다.
pagehide 시 권한을 폐기하며 BFCache 복귀 시에도 이전 grant가 살아나지 않습니다.
동일 문서 동적 DOM의 대상/문맥/상태/모달/뷰포트 검사는 공용 page.js 엔진을 사용합니다.

연결 타임아웃은 해당 프로필 연결을 폐기합니다. 결과를 알 수 없는 조작은 자동 재시도하지 않습니다.
Firefox 재시작과 연결 복구는 새로운 권한으로 시작하며 옛 작업 탭은 인수하지 않습니다.
MV3 초기화는 하나의 Promise로 직렬화하고 native port 및 alarms로 재연결합니다.
업데이트는 로컬 라우터를 종료해 구 native host도 EOF로 끝내고 디스크의 새 실행 파일로 다시
연결하게 합니다. Firefox 자체는 종료하지 않습니다. 새 채팅에서 새 MCP 코드를 사용하세요.

## 배포

일반 Firefox 영구 설치에는 Mozilla 서명이 필요합니다. 임시 확장 재로드 시험과 서명된 확장의
재시작 유지 시험은 구분합니다. [배포 절차](../packaging/SIGNING.md)와 [개인정보 안내](../PRIVACY.md).
BiDi 고급 모드는 별도의 `--bidi` 라우터를 유지합니다. 이전 수동 코드/토큰 확장은
[legacy 문서](extension-legacy-mode.md)에만 보존하며 기본 배포 XPI에 포함하지 않습니다.
