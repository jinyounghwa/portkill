# PRD: PortKill — 경량 포트 관리 도구

## 1. 개요

| 항목 | 내용 |
|------|------|
| **제품명** | PortKill |
| **한줄 요약** | 우분투 리눅스에서 포트 점유 프로세스를 조회하고 즉시 종료할 수 있는 경량 데스크톱 앱 |
| **타겟 환경** | Ubuntu Linux 22.04+ / Intel N100 미니PC / 8GB RAM |
| **기술 스택** | Rust + egui (eframe) |
| **목표 리소스** | 메모리 < 20MB, 바이너리 < 10MB, CPU idle 시 0% |
| **개발 기간** | 2일 (주말 MVP) |
| **라이선스** | MIT |

---

## 2. 문제 정의

개발 서버 운영 중 포트 충돌이 빈번하게 발생한다. 기존 해결 방법은 매번 터미널에서 여러 명령어를 조합하는 것이다.

```bash
# 방법 1
lsof -i :3000
kill -9 <PID>

# 방법 2
sudo netstat -tlnp | grep :3000
sudo kill -9 <PID>
```

**Pain Points:**

- 매번 명령어 조합 필요, PID를 눈으로 찾아 수동 입력
- 여러 포트를 동시에 확인하려면 반복 작업
- 어떤 프로세스가 어떤 포트를 점유하는지 한눈에 파악 불가
- N100급 저사양 미니PC에서 htop/시스템 모니터 류는 과한 리소스 사용

---

## 3. 핵심 원칙

1. **극도의 경량화** — Electron/Tauri WebView 배제, egui 네이티브 렌더링으로 최소 리소스
2. **단일 바이너리** — 외부 의존성 없이 바이너리 복사만으로 설치 완료
3. **원클릭 Kill** — 포트 확인에서 프로세스 종료까지 최소 클릭
4. **권한 인식** — root 권한 필요 시 명확한 UX 안내
5. **Proc 직접 파싱** — 외부 명령어(lsof, netstat) 의존 없이 `/proc` 파일시스템 직접 읽기

---

## 4. 기능 요구사항

### 4.1 MVP (v0.1) — Day 1~2

#### F1. 포트 스캔 및 목록 표시

| 항목 | 상세 |
|------|------|
| **데이터 소스** | `/proc/net/tcp`, `/proc/net/tcp6` 파싱 |
| **필터 조건** | LISTEN 상태 (state = 0x0A) 포트만 기본 표시 |
| **프로세스 정보** | PID → `/proc/{pid}/cmdline`, `/proc/{pid}/status` 에서 추출 |
| **표시 컬럼** | Port, Protocol(TCP/TCP6), PID, Process Name, User, State |
| **정렬** | 포트 번호 오름차순 (기본), 컬럼 클릭 시 정렬 변경 |
| **갱신** | 수동 Refresh 버튼 + 자동 갱신 (5초 주기, 토글 가능) |

#### F2. 포트 검색 및 필터

| 항목 | 상세 |
|------|------|
| **포트 번호 검색** | 상단 입력창에 포트 번호 입력 시 즉시 필터링 |
| **프로세스명 검색** | 프로세스 이름으로 텍스트 검색 |
| **상태 필터** | LISTEN / ESTABLISHED / ALL 토글 |
| **Well-known 표시** | 80(HTTP), 443(HTTPS), 3000, 5432(PG), 3306(MySQL) 등 알려진 포트 라벨 표시 |

#### F3. 프로세스 Kill

| 항목 | 상세 |
|------|------|
| **Kill 버튼** | 각 행 우측에 Kill 버튼 (빨간색) |
| **Kill 시그널** | 기본 SIGTERM(15), Shift+클릭 시 SIGKILL(9) |
| **확인 다이얼로그** | Kill 전 "PID {pid} ({name}) on port {port}를 종료하시겠습니까?" 확인 |
| **권한 처리** | Permission Denied 시 `pkexec` 를 통한 권한 상승 안내 |
| **결과 피드백** | 성공/실패 토스트 메시지 (3초 자동 소멸) |

#### F4. 시스템 트레이 (선택)

| 항목 | 상세 |
|------|------|
| **트레이 아이콘** | 최소화 시 시스템 트레이 상주 |
| **메모리** | 트레이 상주 시 < 5MB |
| **우선순위** | MVP에서는 후순위, 여유 시 구현 |

---

### 4.2 Post-MVP (v0.2+)

| 기능 | 설명 | 우선순위 |
|------|------|----------|
| 포트 감시 모드 | 특정 포트에 새 프로세스 바인딩 시 알림 | P1 |
| 멀티 Kill | 체크박스 선택 후 일괄 종료 | P1 |
| 키보드 단축키 | `/` 검색, `k` Kill, `r` Refresh | P1 |
| 포트 히스토리 | 최근 Kill 한 프로세스 로그 (SQLite) | P2 |
| 다크/라이트 테마 | 시스템 테마 연동 | P2 |
| UDP 포트 지원 | `/proc/net/udp` 파싱 추가 | P2 |
| 원격 서버 | SSH 터널링을 통한 원격 포트 관리 | P3 |

---

## 5. 기술 설계

### 5.1 아키텍처

```
┌─────────────────────────────────────────┐
│              PortKill App               │
├─────────────┬─────────────┬─────────────┤
│   UI Layer  │  Core Logic │  System I/O │
│   (egui)    │             │             │
│             │  Scanner    │  /proc/net  │
│  TableView  │  Filter     │  /proc/pid  │
│  SearchBar  │  Killer     │  kill(2)    │
│  Toast      │  Formatter  │  pkexec     │
└─────────────┴─────────────┴─────────────┘
```

### 5.2 프로젝트 구조

```
portkill/
├── Cargo.toml
├── src/
│   ├── main.rs              # eframe 앱 진입점
│   ├── app.rs               # App 상태 및 UI 루프
│   ├── scanner/
│   │   ├── mod.rs
│   │   ├── proc_parser.rs   # /proc/net/tcp 파싱
│   │   └── process_info.rs  # PID → 프로세스 정보 추출
│   ├── killer/
│   │   ├── mod.rs
│   │   └── signal.rs        # kill 시그널 전송, 권한 처리
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── port_table.rs    # 포트 목록 테이블
│   │   ├── search_bar.rs    # 검색/필터 UI
│   │   └── toast.rs         # 토스트 알림
│   └── models.rs            # PortEntry, ProcessInfo 구조체
└── assets/
    └── icon.png             # 앱 아이콘
```

### 5.3 핵심 데이터 모델

```rust
#[derive(Clone, Debug)]
pub struct PortEntry {
    pub port: u16,
    pub protocol: Protocol,      // TCP | TCP6
    pub state: SocketState,      // Listen | Established | ...
    pub pid: Option<u32>,
    pub process_name: String,
    pub cmdline: String,
    pub user: String,
    pub local_addr: String,
    pub remote_addr: String,
}

#[derive(Clone, Debug)]
pub enum Protocol {
    Tcp,
    Tcp6,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SocketState {
    Established,  // 01
    Listen,       // 0A
    TimeWait,     // 06
    CloseWait,    // 08
    Other(u8),
}
```

### 5.4 /proc/net/tcp 파싱 전략

```
# /proc/net/tcp 포맷 예시
sl  local_address rem_address   st tx_queue rx_queue ...
 0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 ...
    ^^^^^^^^ ^^^^                ^^
    IP(hex)  Port(hex)          State(hex)
```

- `local_address` 에서 `:` 뒤 4자리 hex → port 번호 변환
- `st` 필드: `0A` = LISTEN
- `inode` → `/proc/{pid}/fd/` 심볼릭 링크 매칭으로 PID 역추적
- **또는** `/proc/{pid}/net/tcp` 를 전체 PID 순회하며 매칭 (더 단순)

### 5.5 주요 의존성

```toml
[dependencies]
eframe = "0.31"           # egui 프레임워크 (경량 GUI)
nix = { version = "0.29", features = ["signal"] }  # kill 시그널
users = "0.11"            # UID → username 변환
log = "0.4"
env_logger = "0.11"

[profile.release]
opt-level = "z"           # 바이너리 크기 최소화
lto = true
strip = true
codegen-units = 1
```

### 5.6 성능 목표

| 지표 | 목표 | 측정 방법 |
|------|------|-----------|
| 콜드 스타트 | < 500ms | 바이너리 실행 → 첫 프레임 |
| 메모리 (idle) | < 15MB | `ps aux` RSS |
| 메모리 (active) | < 20MB | 스캔 중 peak RSS |
| 바이너리 크기 | < 10MB | `ls -lh` (release + strip) |
| 포트 스캔 | < 50ms | 100+ 포트 기준 |
| CPU (idle) | 0% | 자동 갱신 OFF 시 |
| CPU (auto-refresh) | < 1% | 5초 주기 갱신 시 |

---

## 6. UI/UX 와이어프레임

```
┌──────────────────────────────────────────────────────┐
│  PortKill                                    [−][□][×]│
├──────────────────────────────────────────────────────┤
│  🔍 [포트 번호 또는 프로세스명 검색...    ]  [↻ Refresh] │
│  ☑ LISTEN  ☐ ESTABLISHED  ☐ ALL    Auto-refresh [ON] │
├──────┬──────┬───────┬────────────┬───────┬───────────┤
│ Port │ Proto│  PID  │ Process    │ User  │ Action    │
├──────┼──────┼───────┼────────────┼───────┼───────────┤
│ 3000 │ TCP  │ 12345 │ node       │ dev   │ [🔴 Kill] │
│ 5432 │ TCP  │  892  │ postgres   │ pg    │ [🔴 Kill] │
│ 8080 │ TCP6 │ 23456 │ java       │ dev   │ [🔴 Kill] │
│ 3306 │ TCP  │  1023 │ mysqld     │ mysql │ [🔴 Kill] │
│ 6379 │ TCP  │  2048 │ redis-srv  │ redis │ [🔴 Kill] │
├──────┴──────┴───────┴────────────┴───────┴───────────┤
│  Total: 5 ports listening                            │
│  ✅ Process node (PID 12345) killed successfully     │
└──────────────────────────────────────────────────────┘
```

---

## 7. 빌드 및 배포

### 7.1 빌드

```bash
# 개발
cargo run

# 릴리즈 (최적화 빌드)
cargo build --release

# 바이너리 위치
./target/release/portkill
```

### 7.2 설치

```bash
# 단일 바이너리 복사
sudo cp target/release/portkill /usr/local/bin/

# 실행
portkill

# 또는 권한 상승이 필요한 경우
sudo portkill
```

### 7.3 .desktop 파일 (선택)

```ini
[Desktop Entry]
Name=PortKill
Comment=Lightweight Port Manager
Exec=/usr/local/bin/portkill
Icon=portkill
Type=Application
Categories=Development;System;
```

---

## 8. 권한 및 보안

| 시나리오 | 동작 |
|----------|------|
| 일반 유저 실행 | 본인 소유 프로세스만 Kill 가능, 다른 유저 프로세스는 포트 정보만 표시 |
| root 실행 | 모든 프로세스 Kill 가능 |
| Permission Denied | 토스트로 "권한 부족: `sudo portkill` 로 재실행하세요" 안내 |
| 시스템 프로세스 Kill 시도 | PID 1(systemd) 등 보호 프로세스는 Kill 버튼 비활성화 |

---

## 9. 개발 일정

### Day 1 (토요일) — 코어 + 기본 UI

| 시간 | 작업 |
|------|------|
| 2h | 프로젝트 셋업, `/proc/net/tcp` 파서 구현 및 테스트 |
| 2h | PID → 프로세스 정보 매핑, kill 시그널 모듈 |
| 2h | egui 기본 UI: 테이블 뷰, 검색창 |
| 1h | Kill 버튼 연동, 확인 다이얼로그 |

### Day 2 (일요일) — 완성도 + 릴리즈

| 시간 | 작업 |
|------|------|
| 2h | 필터링 (LISTEN/ESTABLISHED/ALL), 정렬, 자동 갱신 |
| 1h | 토스트 알림, 에러 핸들링 |
| 1h | Well-known 포트 라벨, UI 폴리싱 |
| 1h | 릴리즈 빌드, README 작성 |
| 1h | 테스트 및 버그 수정 |

---

## 10. 성공 지표

| 지표 | 목표 |
|------|------|
| 바이너리 크기 | < 10MB |
| 메모리 사용량 | < 20MB |
| 포트 스캔 → Kill 까지 클릭 수 | ≤ 3회 (검색 → Kill → 확인) |
| N100에서 체감 렉 | 없음 |
| GitHub 스타 (3개월) | 50+ |

---

## 11. 리스크 및 대응

| 리스크 | 확률 | 대응 |
|--------|------|------|
| egui 테이블 렌더링 성능 | 낮음 | 가상 스크롤 적용, 표시 행 수 제한 |
| /proc 파싱 호환성 (커널 버전) | 낮음 | Ubuntu 22.04+ 타겟, 파싱 실패 시 fallback 메시지 |
| 권한 문제로 PID 정보 누락 | 중간 | 누락 시 "Unknown" 표시, sudo 실행 권장 안내 |
| egui 한글 렌더링 | 중간 | NotoSansKR 폰트 임베딩 또는 영문 UI 우선 |

---

## 12. 향후 확장 가능성

- **TUI 버전**: `ratatui` 기반 터미널 UI 버전 (SSH 접속 시 유용)
- **시스템 트레이 상주**: 특정 포트 감시 + 데스크톱 알림
- **플러그인**: 포트별 커스텀 액션 (restart 서비스 등)
- **크로스 플랫폼**: macOS `/proc` 대신 `lsof` fallback

---

*최종 수정: 2026-02-10*
