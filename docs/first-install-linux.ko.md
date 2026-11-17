# Linux에서 처음 설치하기

[English](first-install-linux.md) · **한국어 번역**

대상은 Linux x86_64, 일반 데스크톱 Firefox 140+와 로컬 Codex 앱/CLI입니다.
Codex에 로그인할 수 있어야 합니다. Rust·Node·프로젝트 소스는 필요 없습니다.
Snap/Flatpak Firefox와 다른 운영체제의 설치 흐름은 이 안내의 검증 범위 밖입니다.

**Linux 0.4.1은 영문 정식판입니다.** 설치 화면과 확장은 영어이며 아래 버튼 이름은
실제 화면과 같습니다. Windows는 별도 프리뷰이고 과거 배포 파일은 그대로 남깁니다.

## 배포 파일 실행

1. [Linux 0.4.1 압축 파일](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.1/firefox-codex-mcp-0.4.1-linux-x86_64.tar.gz)을 받습니다.
2. 파일 관리자의 **압축 풀기**로 내용을 모두 꺼냅니다. 압축 파일 안에서 실행하지 않습니다.
3. 풀린 폴더를 열어 **여기서 터미널 열기**를 선택하고 다음 한 줄을 실행합니다.

   ```sh
   ./install.sh
   ```

파일 관리자에 따라 스크립트가 편집기로 열리거나 실행 여부를 물을 수 있습니다.
편집기로 열리면 위 터미널 방식을 사용하세요. 관리자 권한으로 실행할 필요는 없습니다.
현재 Linux 배포는 파일 관리자 종류와 무관한 더블클릭 설치를 보장하지 않습니다.

## 연결과 확장 설치

Firefox에 열린 관리 화면에서 **Install connection → Continue**를 누릅니다. 연결 프로그램,
Codex의 MCP 등록, `firefox-browser` 스킬과 사용 지침을 설치기가 자동으로 준비합니다.
스킬을 따로 내려받거나 설정 파일을 편집하지 않습니다.

같은 화면의 **Add Codex Work Tabs to Firefox**를 누르고 Firefox 안내의
**설치 계속/허용 → 추가**를 확인합니다. 연결 프로그램과 Firefox 확장은 둘 다 필요합니다.
여러 프로필이 연결되면 사용할 Firefox의 확장 버튼에서 **Use this Firefox**를 선택합니다.

## 첫 웹 작업 확인

Codex 앱 또는 대화형 CLI에서 **새 채팅**을 열고 다음처럼 요청합니다.

> https://example.com을 브라우저로 열고, Learn more 링크를 눌러 이동한 페이지 제목을 알려줘.

새 Firefox 작업 탭이 열리고 링크를 눌러 이동한 결과를 알려주면 첫 연결이 확인됩니다.
도구 사용 승인 요청은 자신이 요청한 웹 작업인지 확인한 뒤 허용합니다.
관리 화면의 **Registered** 표시는 설정 확인이며 실제 탭 제어 성공과는 구분합니다.

## 막히는 경우

| 보이는 현상 | 확인할 것 |
| --- | --- |
| 스크립트가 편집기로 열림 | 압축을 푼 폴더의 터미널에서 `./install.sh` 실행 |
| Codex를 찾지 못함 | Codex 앱/CLI 설치 상태를 확인하고 관리 화면을 다시 열기. Linux 설치기는 Codex 실행 파일을 찾아야 함 |
| 도구가 안 보임 | Codex 앱을 다시 열고 새 채팅에서 요청 |
| 확장이 중지 상태 | 사용할 Firefox 확장 버튼에서 **Resume** 선택 |
| 여러 프로필 중 선택 필요 | 원하는 Firefox의 **Use this Firefox** 선택 |
| 도구 승인이 거부됨 | 호스트가 제공하는 승인 절차 사용. 설치기는 사용자·관리자의 승인 정책을 바꾸지 않음 |

비대화형 `codex exec`는 일반 채팅과 승인 방식이 다릅니다. 실제 시험에서도 기본
승인 거부 정책에서는 새 탭 열기가 차단됐습니다. 처음에는 Codex 앱 또는 대화형 CLI로
확인하세요. 자동화하는 개발자는 자신의 Codex 버전이 지원하는 승인 검토 방식을 별도로
정해야 하며, 이 도구를 쓰기 위해 전체 승인이나 샌드박스를 해제할 필요는 없습니다.

## 검증 범위

[0.4.1 릴리스](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.1)에 실제
배포 파일의 체크섬·GitHub 빌드 증명·`release-verification.json`을 제공합니다.
[영문 기본 화면 검증 기록](english-default-e2e-2026-11-17.json)은 그전에 시험한 로컬 후보
빌드의 역사적 기록이며 해당 파일과 검증 범위를 별도로 표시합니다.

[2026-11-17 새 환경 검증 기록](linux-first-install-e2e-2026-11-17.json)을 확인하세요.
공개 배포 파일을 프로젝트 밖에서 실행했고, 기존 홈을 가린 임시 홈에서 설정이 없는
상태로 시작했습니다. Codex 로그인만 읽기 전용으로 연결했습니다.
Firefox의 정상 확장 설치 확인을 거쳐 실제 모델의 읽기·클릭·입력을 확인했습니다.

브라우저는 자동화를 위한 headless/Marionette 환경입니다. 실제 새 PC·새 OS 설치,
파일 관리자 더블클릭 동작이나 초보 사용자가 혼자 수행하는 사용성 시험의 대체는 아닙니다.
호스트에 설치된 Codex·Firefox와 시스템 라이브러리를 사용했으며 이 전제 프로그램들의
설치나 신규 계정 로그인 과정은 시험하지 않았습니다. 확장 설치 확인은 실제로 표시된
버튼을 자동화로 눌렀고, 모델 CLI에는 실행할 때만 자동 승인 검토를 사용했습니다.
일반 사용자의 수동 승인 화면과 동일한 사용성이라고 보장하지 않습니다.
이 문서와 관리 화면의 첫 작업 예시는 이번 검토에서 보완했으며, 기존 공개 0.4.0 파일을
교체한 것은 아닙니다.

## 개발자의 재현 방법

다음은 배포 사용자에게 필요한 절차가 아니라 회귀 시험 실행 방법입니다.
개발 환경에는 Rust, `bwrap`, geckodriver와 실제 모델에 연결할 수 있는 Codex가 필요합니다.
배포 압축은 프로젝트 밖의 `/tmp` 아래에 풀고, 매번 새 산출물 폴더를 지정합니다.

```sh
FIREFOX_FIRST_RUN_PACKAGE=/tmp/extracted/firefox-codex-mcp-VERSION-linux-x86_64 \
FIREFOX_FIRST_RUN_ARTIFACTS=/tmp/firefox-first-run-new-output \
FIREFOX_FIRST_RUN_CODEX=/absolute/path/to/native/codex \
FIREFOX_GECKODRIVER=/absolute/path/to/geckodriver \
cargo test --release --locked --test firefox_e2e \
  first_run_e2e::clean_distribution_first_web_task -- \
  --exact --ignored --nocapture --test-threads=1
```

시험이 직접 임시 홈과 PID/mount namespace를 만듭니다. 기존 홈을 가리고 시스템을
읽기 전용으로 두며, `.codex/auth.json`만 읽기 전용으로 연결합니다. 로그인 자격 증명을
별도 파일에 복사하지 않습니다. 현재 시험은 이 파일 기반 로그인 방식이 필요합니다.
기존 설정의 전후 해시를 비교하고 시험용 계정 폴더와 프로세스를 정리합니다.

빈 Codex 설정에서 배포 스크립트를 인수 없이 실행하고, 스크립트가 띄운 Firefox에
WebDriver를 연결합니다. 실제 확장 설치 알림을 거쳐 설정을 자동 설치한 뒤,
도구 이름을 지정하지 않은 영어 웹 작업을 실제 모델에 요청합니다. 별도의 DOM
읽기로 클릭 횟수와 입력값을 확인합니다. MCP 명령·스킬·모델을 시험용으로 덮어쓰지
않으며, CLI 실행에만 `--approve-for-me`를 사용합니다.

`first-run.json`, `isolation.json`, 모델 도구 호출 원문과 단계별 PNG가 지정 폴더에
남습니다. 실행 시간은 자동화 경과 시간이며 초보자의 설치 소요 시간은 아닙니다.
