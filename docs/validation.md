# 검증 기록

2026-11-16 개발 환경 기준:

- Rust stable 1.100.0: `cargo fmt --all -- --check` 통과.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` 통과.
- `cargo test --workspace --locked`: 5개 통과. Origin/token/인자 검증,
  MCP 초기화 및 연결 없음 오류, 모의 확장 WebSocket 인증/요청/응답 왕복 포함.
- Node `node --test extension/tests/*.test.cjs`: 14개 통과.
- `node --check extension/*.js`: 각 JavaScript 파일 문법 확인 통과.

위 결과는 단위/모의 연동 검증입니다. **실제 Firefox 확장 로드, activeTab 권한,
Firefox Origin/CSP, 로그인된 사이트 동작, Codex와의 실사용은 검증하지 않았습니다.**
CI 설정은 포함했지만 원격 실행 결과는 별도로 확인해야 합니다.

## 실사용 전 수동 검증

1. 테스트용 페이지와 임시 Firefox 프로필에서 확장을 로드합니다.
2. 잘못된 토큰 연결은 거절되고, 올바른 토큰 연결만 허용되는지 봅니다.
3. 탭 허용 전 snapshot은 실패하는지 확인합니다.
4. 명시적으로 허용한 뒤 snapshot → click/type/scroll을 하나씩 시험합니다.
5. DOM 변경 뒤 이전 snapshotId가 거절되는지 확인합니다.
6. 페이지 이동·탭 닫기·Disconnect 후 이전 탭의 명령이 거절되는지 봅니다.
7. 일반 웹페이지에서 브리지에 연결하려는 요청이 Origin 검사로 거절되는지 봅니다.
8. Codex `firefox_status`와 snapshot 출력을 확인합니다. 실제 계정 조작은 그 다음입니다.
