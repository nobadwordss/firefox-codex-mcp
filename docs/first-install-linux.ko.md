# Linux에서 처음 설치하기

[English](first-install-linux.md) · **한국어 번역**

대상은 Linux x86_64, 일반 데스크톱 Firefox 140+와 로컬 Codex 앱/CLI입니다.
첫 작업 전에 Codex에 로그인할 수 있어야 합니다. Rust·Node·프로젝트 소스는 필요 없습니다.
Snap/Flatpak Firefox와 다른 운영체제는 이 안내의 검증 범위 밖입니다.

**Linux 0.4.3은 영문 정식판입니다.** RPM·DEB를 기본으로 배포하며 관리자 권한이 없으면
보조 tar.gz를 사용합니다. 관리 화면과 Mozilla 서명 확장 **0.4.2**는 영어입니다.
기존 Firefox 프로필·로그인·탭을 유지하며 별도 앱 메뉴 항목이나 프로필은 만들지 않습니다.
Windows **0.5.0-beta.3**는 별도 프리뷰로 그대로 유지합니다.

## 시스템 연결 프로그램 설치

배포판에 맞는 파일을 받습니다.

| 배포판 | 다운로드 |
| --- | --- |
| Fedora | [firefox-codex-mcp-0.4.3-1.x86_64.rpm](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp-0.4.3-1.x86_64.rpm) |
| Ubuntu / Debian | [firefox-codex-mcp_0.4.3-1_amd64.deb](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp_0.4.3-1_amd64.deb) |

로컬 패키지를 지원하는 소프트웨어 관리자로 엽니다. 설치 화면이 열리지 않으면 다운로드
폴더의 터미널에서 배포판에 맞는 명령 하나를 실행합니다.

```sh
# Fedora
sudo dnf install ./firefox-codex-mcp-0.4.3-1.x86_64.rpm
```

```sh
# Ubuntu / Debian
sudo apt install ./firefox-codex-mcp_0.4.3-1_amd64.deb
```

이 단계는 시스템 연결 프로그램만 설치합니다. Firefox를 열거나 root의 Codex를 등록하지
않고 사용자 홈도 수정하지 않습니다. 이후에는 일반 사용자로 진행하며 **Connect Codex를
sudo로 실행하지 않습니다.** 로컬 패키지는 자동 APT/DNF 업데이트 저장소를 추가하지 않습니다.
RPM·DEB에 별도 패키지·저장소 서명은 없고 XPI는 Mozilla 서명입니다. 파일을 확인할 때
릴리스의 `SHA256SUMS`와 GitHub 빌드 증명을 함께 확인하세요.

## 확장 설치와 계정 연결

1. 일반 Firefox에서 [Mozilla 서명 Codex Work Tabs 0.4.2 XPI](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/codex-work-tabs-0.4.2.xpi)를 엽니다.
   Firefox의 **설치 계속/허용 → 추가** 안내를 확인합니다. 파일로 저장되면 부가 기능
   관리자 → 톱니바퀴 → **파일에서 부가 기능 설치**로 XPI를 선택합니다.
2. **Codex Work Tabs → Settings → Connect Codex → Continue**를 누릅니다.
   이 계정의 Codex 연결·브라우저 스킬·사용 지침을 자동 등록합니다.
   스킬을 따로 받거나 설정 파일을 편집하지 않습니다.
3. 연결할 OS 계정마다 **Connect Codex**를 진행하고, 평소 사용하는 Firefox 프로필에
   확장을 설치합니다.

확장이 이미 설치됐다면 RPM/DEB 설치 후 확장의 **Settings**를 열면 됩니다.
Settings가 열리지 않으면 일반 사용자로 `firefox-codex-mcp --manage`를 실행합니다.
관리 화면의 **Add Codex Work Tabs to Firefox**로도 확장을 설치할 수 있습니다.
여러 프로필이 연결되면 원하는 Firefox의 **Use this Firefox**를 선택합니다.

## 첫 웹 작업 확인

Codex 앱 또는 대화형 CLI에서 **새 채팅**을 열고 다음처럼 요청합니다.

> https://example.com을 브라우저로 열고, Learn more 링크를 눌러 이동한 페이지 제목을 알려줘.

새 작업 탭이 열리고 링크를 눌러 결과를 알려주면 실제 연결이 확인됩니다.
도구 승인 요청은 자신이 요청한 웹 작업인지 확인한 뒤 허용합니다. 관리 화면의
**Registered**는 설정 확인이며 실제 탭 제어 성공과는 구분합니다.

## 업데이트·연결 해제·제거

새 RPM/DEB를 소프트웨어 관리자나 새 파일명을 사용한 같은 명령으로 설치합니다.
각 계정은 시스템 프로그램을 직접 사용합니다. 업데이트·제거하면 이전 패키지의 실행 중인
연결 프로그램을 중단하고 작업 탭 접근을 해제합니다. Firefox와 결과 탭은 유지하며,
업데이트 후에는 새 Codex 채팅에서 시작합니다.

완전히 제거하려면 다음 순서로 진행합니다.

1. 연결한 OS 계정마다 **Codex Work Tabs → Settings → Disconnect Codex**를 누릅니다.
   해당 계정의 소유한 연결·스킬·지침만 제거하고 사용자 수정은 보존합니다.
   시스템 프로그램은 남습니다.
2. 소프트웨어 관리자 또는 `sudo dnf remove firefox-codex-mcp` /
   `sudo apt remove firefox-codex-mcp`로 시스템 패키지를 제거합니다. DEB purge도 지원합니다.
3. 원하면 Firefox 부가 기능 관리자에서 확장을 별도로 제거합니다.

패키지부터 제거해도 실행 중인 패키지 연결 프로그램은 중단하지만 사용자 설정은 남깁니다.
RPM/DEB를 재설치해 각 계정에서 Disconnect하거나, 현재 tar.gz를 풀어
`./install.sh --uninstall`로 남은 연결 장부를 정리할 수 있습니다. Firefox 프로필이나
Codex 설정 폴더 전체를 지우지 마세요. [이전과 복구](linux-packages.md)를 참고하세요.
릴리스가 새 서명 XPI를 제공하면 별도로 설치합니다. 직접 배포 XPI에는 자동 확장 업데이트
채널이 없습니다.

## 관리자 권한이 없을 때: tar.gz 보조 배포

[0.4.3 tar.gz](https://github.com/nobadwordss/firefox-codex-mcp/releases/download/v0.4.3/firefox-codex-mcp-0.4.3-linux-x86_64.tar.gz)를
완전히 풀고 해당 폴더의 터미널에서 실행합니다.

```sh
./install.sh
```

**Install connection → Continue → Add Codex Work Tabs to Firefox**를 선택하고 Firefox의
안내를 확인합니다. 이 방식은 OS 계정의 개인 연결 프로그램을 설치합니다. 이후 제거는
**Settings → Remove connection** 또는 `./install.sh --uninstall`, 업데이트는 새 압축 파일의
**Update connection**입니다. 스크립트가 편집기로 열리면 위 터미널 방식을 쓰세요.
파일 관리자마다 더블클릭 동작이 같다고 보장하지 않습니다.

RPM/DEB가 이미 설치됐다면 별도 개인 복사본 대신 시스템 연결 프로그램에 연결합니다.
이전 tar.gz에서 옮기려면 RPM/DEB를 설치하고 **Reconnect Codex**를 선택합니다.
변경되지 않은 장부 소유의 파일만 정리하며, 시스템 등록을 가리는 수정·외부 등록은
보존하고 충돌을 알립니다. [관리 안내](firefox-only-management.md)를 참고하세요.

## 막히는 경우

| 현상 | 확인할 것 |
| --- | --- |
| 패키지 설치 화면이 열리지 않음 | 다운로드 폴더에서 위의 dnf/apt 명령 사용 |
| Settings가 연결 프로그램에 연결하지 못함 | `firefox-codex-mcp --manage` 실행. 명령이 없으면 RPM/DEB 재설치 |
| 개인 Native Messaging 등록 충돌 | 보고된 파일을 검토한 뒤 직접 옮기기. 수정·외부 파일은 자동 삭제하지 않음 |
| 도구가 안 보임 | 로컬 Codex 설치·로그인 확인 후 앱을 다시 열어 새 채팅에서 요청. 설치 시 CLI가 PATH에 없어도 됨 |
| 확장이 중지됨 | 원하는 Firefox의 **Resume** 선택 |
| 여러 프로필 선택 필요 | 원하는 Firefox의 **Use this Firefox** 선택 |
| 도구 승인 거부 | 호스트가 허용하는 승인 절차 사용. 설치기는 승인 정책을 바꾸지 않음 |

비대화형 `codex exec`는 일반 채팅과 승인 방식이 다릅니다. 첫 검증에서는 기본 승인 거부
정책이 새 탭을 차단했습니다. 처음에는 Codex 앱 또는 대화형 CLI로 확인하세요.
자동화 개발자는 자신의 Codex 버전이 지원하는 승인 검토 방식을 선택해야 하며,
이 도구를 위해 전체 승인이나 샌드박스를 해제할 필요는 없습니다.

## 검증 범위

[0.4.3 릴리스](https://github.com/nobadwordss/firefox-codex-mcp/releases/tag/v0.4.3)는
정확한 RPM·DEB·압축 배포 파일별 체크섬·GitHub 빌드 증명·`release-verification.json`을
제공합니다. 패키지 수명주기 CI는 폐기 가능한 Ubuntu·Fedora 환경에서 수행하며,
[패키지 소유권과 검증](linux-packages.md)을 참고하세요. 자동 시험은 초보자나 새 데스크톱
OS에서의 수동 설치 검증과 구분합니다.

[Firefox 관리 구조의 소스 검증 기록](firefox-only-management-e2e-2026-11-17.json)은 해당 소스 빌드,
서명 확장과 깨끗한 홈의 실제 모델 작업을 기록합니다. 최종 배포 파일의 검증 기록은 별도입니다.
[영문 기본 화면 검증 기록](english-default-e2e-2026-11-17.json)은 이전 로컬 후보 빌드의
역사적 기록입니다.

[2026-11-17 새 환경 검증 기록](linux-first-install-e2e-2026-11-17.json)을 확인하세요.
공개 0.4.0 압축 배포 파일을 프로젝트 밖에서 실행했고, 기존 홈을 가린 임시 홈에서 설정이 없는
상태로 시작했습니다. Codex 로그인만 읽기 전용으로 연결했습니다.
Firefox의 정상 확장 설치 확인을 거쳐 실제 모델의 읽기·클릭·입력을 확인했습니다.

브라우저는 자동화를 위한 headless/Marionette 환경입니다. 실제 새 PC·새 OS 설치,
파일 관리자 더블클릭 동작이나 초보 사용자가 혼자 수행하는 사용성 시험의 대체는 아닙니다.
호스트에 설치된 Codex·Firefox와 시스템 라이브러리를 사용했으며 이 전제 프로그램들의
설치나 신규 계정 로그인 과정은 시험하지 않았습니다. 확장 설치 확인은 실제로 표시된
버튼을 자동화로 눌렀고, 모델 CLI에는 실행할 때만 자동 승인 검토를 사용했습니다.
일반 사용자의 수동 승인 화면과 동일한 사용성이라고 보장하지 않습니다.
과거 0.4.0·0.4.1 배포 파일과 각 검증 기록은 변경하지 않습니다.

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
