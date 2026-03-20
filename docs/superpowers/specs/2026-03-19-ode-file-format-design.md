# `.ode` File Format Design

> "You don't have to know design." — and you don't have to know file formats either.

## Overview

Open Design Engine의 파일 포맷을 `.ode.json` (순수 JSON)에서 `.ode` (ZIP 컨테이너)로 전환한다. `.ode`는 단일 파일로 자기완결적이며, `ode unpack`으로 풀어서 에이전트가 JSON을 직접 편집할 수 있다.

### 목표

1. **독자 포맷 아이덴티티**: `.ode` 확장자 — Figma의 `.fig`, Sketch의 `.sketch`처럼
2. **에이전트 친화성**: `unpack` 후 `document.json`을 `Read`/`Edit`으로 직접 조작
3. **단일 파일 자기완결성**: 이미지 에셋을 ZIP 내부에 포함 — 공유 시 파일 하나
4. **하위 호환**: 기존 `.ode.json` 파일을 자동 인식하여 읽기

### Non-goals

- 바이너리 포맷 (MessagePack, FlatBuffers 등)
- 증분 저장 (incremental save)
- 암호화/DRM

---

## 1. 파일 구조

### 1.1 ZIP 내부 레이아웃 (Packed)

```
design.ode (ZIP)
├── document.json       ← 노드 트리, 토큰, 뷰 (기존 .ode.json 내용)
├── meta.json           ← 컨테이너 메타데이터
└── assets/
    ├── a1b2c3d4e5f6.png
    └── f7e8d9c0b1a2.jpg
```

- ZIP 압축 방식: `document.json`과 `meta.json`은 **Deflate**, `assets/` 내 이미지는 **Store** (PNG/JPEG/WebP 등 이미 압축된 포맷). SVG, BMP 등 비압축 에셋은 Deflate 적용.
- ZIP 내 경로는 항상 `/` 구분자, 루트에 `document.json` 위치

### 1.2 풀린 디렉토리 레이아웃 (Unpacked)

```
design/
├── document.json
├── meta.json
└── assets/
    ├── a1b2c3d4e5f6.png
    └── f7e8d9c0b1a2.jpg
```

디렉토리 이름은 원본 `.ode` 파일명에서 확장자를 제거한 것.

### 1.3 `meta.json`

```json
{
  "format_version": "1.0.0",
  "generator": "ode-cli 0.1.0",
  "created_at": "2026-03-19T12:00:00Z",
  "modified_at": "2026-03-19T14:30:00Z"
}
```

| 필드 | 설명 |
|------|------|
| `format_version` | 컨테이너 포맷 버전 (semver). `document.json` 내 `format_version`과 독립 |
| `generator` | 생성 도구 이름과 버전 |
| `created_at` | 최초 생성 시각 (ISO 8601) |
| `modified_at` | 마지막 수정 시각 (ISO 8601) |

### 1.4 이미지 참조

`document.json` 내에서 이미지를 참조할 때:

```json
{
  "type": "linked",
  "path": "assets/a1b2c3d4e5f6.png"
}
```

- 경로는 항상 **상대 경로** (ZIP 루트 또는 풀린 디렉토리 기준)
- 파일명은 **SHA-256 앞 16자리 hex** (64-bit) + 원본 확장자 — 동일 이미지 중복 방지
  - 예: `SHA-256(bytes) = a1b2c3d4e5f67890abcd...` → `a1b2c3d4e5f67890.png`
  - 해시 충돌 시: `add_image()`가 기존 에셋과 바이트 비교, 불일치하면 suffix 추가 (`_2`)
  - 동일 콘텐츠의 동일 해시는 중복 제거 (dedup) — 에셋 하나만 저장

### 1.5 `ImageSource::Embedded` 처리

- `Embedded`는 **런타임 전용** (메모리 내에서만 존재)
- `.ode` 저장 시 → `assets/`에 추출, `Linked`로 교체
- `.ode` 로드 시 → `Linked` 유지 (렌더러가 `AssetStore`를 통해 읽음)
- `Embedded` variant는 코드에서 제거하지 않음 — Figma 임포트 등 파이프라인에서 임시로 사용

---

## 2. CLI 인터페이스

### 2.1 입력 경로 투명 처리

모든 CLI 명령이 `.ode` 파일(ZIP), 풀린 디렉토리, stdin을 자동 구분:

```bash
ode build design.ode          # ZIP → document.json 추출 → 렌더
ode build design/             # 디렉토리 → document.json 직접 읽기
ode validate design.ode       # 둘 다 동일
cat doc.json | ode validate - # stdin (기존 호환)
```

판별 로직:

```
입력 경로
  ├─ "-"                         → Stdin (JSON 직접)
  ├─ 디렉토리                    → Unpacked
  ├─ .ode 확장자 + ZIP magic     → Packed
  └─ 그 외 (.ode.json 등)       → Legacy JSON
```

### 2.2 새 명령: `ode pack` / `ode unpack`

```bash
ode pack design/ --output design.ode     # 디렉토리 → .ode
ode unpack design.ode --output design/   # .ode → 디렉토리

# output 생략 시 기본 규칙:
ode pack design/        → design.ode   (같은 위치)
ode unpack design.ode   → design/      (같은 위치)
```

출력 대상이 이미 존재하면 덮어쓰기 (기본 동작). 에이전트 워크플로우에서 반복 pack/unpack이 자연스러워야 하므로 별도 `--force` 플래그 없이 덮어쓴다.

### 2.3 `ode new` 변경

```bash
ode new design.ode --width 1920 --height 1080   # .ode ZIP 생성
ode new design/ --width 1920 --height 1080       # 풀린 디렉토리 생성
```

출력 경로가 `/`로 끝나거나 이미 디렉토리이면 → Unpacked 생성. 그 외 → Packed 생성.

### 2.4 저장 명령 동작

`ode set`, `ode add`, `ode delete`, `ode move`:

- **입력이 디렉토리** → `document.json` 직접 수정
- **입력이 `.ode` 파일** → ZIP 열기 → 수정 → ZIP 다시 쓰기

### 2.5 Figma 임포트

```bash
ode import figma --token $TOKEN --file-key abc --output design.ode    # Packed
ode import figma --token $TOKEN --file-key abc --output design/       # Unpacked
```

다운로드한 이미지 → `assets/`에 해시 파일명으로 저장, `Embedded` → `Linked` 변환.

### 2.6 도움말/문서 업데이트

모든 `.ode.json` 참조 → `.ode`로 변경 (clap 도움말, 에러 메시지).

---

## 3. `ode-format` 크레이트 변경

### 3.1 새 모듈: `container.rs`

```rust
/// .ode 파일 또는 풀린 디렉토리를 추상화
pub enum OdeSource {
    Packed(PathBuf),
    Unpacked(PathBuf),
    Stdin,
    LegacyJson(PathBuf),
}

pub struct OdeContainer {
    pub document: Document,
    pub meta: Meta,
    pub assets: AssetStore,
    source: OdeSource,
}

impl OdeContainer {
    /// 경로에서 자동 판별하여 열기
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ContainerError>;

    /// 원래 소스에 저장 (Packed → ZIP, Unpacked → 디렉토리)
    /// Stdin 소스일 경우 에러 반환 (save_packed/save_unpacked 사용 필요)
    pub fn save(&mut self) -> Result<(), ContainerError>;

    /// 지정 경로에 Packed(.ode)로 저장
    pub fn save_packed(&mut self, path: &Path) -> Result<(), ContainerError>;

    /// 지정 경로에 Unpacked(디렉토리)로 저장
    pub fn save_unpacked(&mut self, path: &Path) -> Result<(), ContainerError>;
}
```

### 3.2 `AssetStore`

```rust
pub struct AssetStore {
    entries: HashMap<String, AssetEntry>,
    dir: Option<PathBuf>,
}

enum AssetEntry {
    OnDisk(PathBuf),
    Loaded(Vec<u8>),
}

impl AssetStore {
    /// 이미지 바이트 등록. SHA-256 해시 파일명 반환.
    pub fn add_image(&mut self, bytes: Vec<u8>, ext: &str) -> String;

    /// 해시로 이미지 바이트 가져오기 (lazy load)
    pub fn get_image(&mut self, hash: &str) -> Result<&[u8], AssetError>;

    /// 에셋의 디스크 경로 반환
    pub fn resolve_path(&self, hash: &str) -> Option<PathBuf>;
}
```

### 3.3 `Meta`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub format_version: Version,
    pub generator: String,
    pub created_at: String,
    pub modified_at: String,
}

impl Meta {
    pub fn new(generator: &str) -> Self;
}
```

### 3.4 저장 흐름

```
Document (메모리)
  │
  ├─ 모든 노드 순회 → Embedded 이미지 추출
  │   ├─ ImageData.source (이미지 노드)
  │   └─ Paint::ImageFill { source } (모든 노드의 fills/strokes)
  │   각각에 대해:
  │   ├─ SHA-256 해시 계산 → 해시 파일명 생성
  │   ├─ AssetStore.add_image(bytes, ext)
  │   └─ Embedded → Linked { path: "assets/{hash}.{ext}" } 교체
  │
  ├─ meta.modified_at 갱신
  ├─ document.json 직렬화
  ├─ meta.json 직렬화
  │
  └─ OdeSource에 따라:
      ├─ Packed → 임시 파일에 ZIP 쓰기 → rename (원자적 교체)
      └─ Unpacked → 파일 쓰기
```

**원자적 저장 (Packed 모드)**: ZIP을 같은 디렉토리의 임시 파일에 쓴 뒤 `rename()`으로 교체한다. 쓰기 중 크래시 시 원본 `.ode` 파일이 손상되지 않는다.

### 3.5 로드 흐름

```
입력 경로
  │
  ├─ OdeSource 판별
  │
  ├─ Packed → ZIP에서 document.json, meta.json 추출
  ├─ Unpacked → 디렉토리에서 직접 읽기
  ├─ LegacyJson → JSON 파싱 (meta 자동 생성: format_version "1.0.0",
  │                generator "ode-format (legacy)", timestamps from file mtime)
  ├─ Stdin → JSON 파싱 (에셋 없음, meta 자동 생성)
  │
  ├─ document.json → Document 역직렬화
  ├─ meta.json → Meta 역직렬화
  └─ assets/ → AssetStore 등록 (lazy, 바이트 미로드)
```

---

## 4. 크레이트별 영향 범위

| 크레이트 | 변경 내용 |
|---------|----------|
| `ode-format` | `container.rs` 추가 (`OdeContainer`, `AssetStore`, `Meta`). `ImageSource::Embedded` 유지 |
| `ode-core` | `Scene::from_document(&doc, &font_db, &asset_store)` — 새 매개변수 추가. `convert.rs`의 `emit_image()`가 `AssetStore::get_image()`로 바이트 조회. 기존 `std::fs::read(path)` 직접 호출 제거 |
| `ode-cli` | `load_input()` → `OdeContainer::open()` 교체. `pack`/`unpack` 명령 추가. 도움말 문자열 업데이트 |
| `ode-import` | Figma 이미지 → `AssetStore::add_image()` 사용. `Embedded` 직접 생성 대신 `AssetStore` 경유 |
| `ode-export` | 변경 없음 (Scene IR만 받음) |
| `ode-review` | 변경 없음 (Document만 받음) |

### 새 의존성

| 크레이트 | 추가 의존성 |
|---------|------------|
| `ode-format` | `zip` (ZIP 읽기/쓰기), `sha2` (해시 계산) |

---

## 5. 하위 호환 및 마이그레이션

### 5.1 기존 `.ode.json` 자동 인식

`OdeSource` 판별에서 `.ode.json` 확장자 또는 ZIP magic이 아닌 파일 → Legacy JSON으로 처리. 기존 동작과 완전 동일 (`Embedded` 이미지 포함 파싱).

### 5.2 마이그레이션 경로

```bash
# 기존 파일을 새 포맷으로 변환
ode pack old-design.ode.json --output new-design.ode
```

Legacy JSON 로드 시 `Embedded` 이미지가 있으면, `save_packed()` 또는 `save_unpacked()` 시 자동으로 `assets/`로 추출.

### 5.3 포맷 버전 전략

| 버전 | 위치 | 의미 |
|------|------|------|
| `document.json` 내 `format_version: 0.2.0` | 데이터 모델 | 노드 타입, 속성 추가 시 올림 |
| `meta.json` 내 `format_version: 1.0.0` | 컨테이너 | 파일 구조 변경 시 올림 |

두 버전은 독립적으로 진화.

### 5.4 테스트 전략

기존 통합 테스트를 세 가지 경로로 확장:

```rust
#[test] fn from_packed_ode() { ... }       // ZIP .ode 파일
#[test] fn from_unpacked_dir() { ... }     // 풀린 디렉토리
#[test] fn from_legacy_json() { ... }      // 기존 .ode.json
```

---

## 6. 에이전트 워크플로우

### 6.1 새 디자인 생성

```bash
ode new design/ --width 1920 --height 1080    # 풀린 형태 생성
# → 에이전트가 document.json 직접 편집
ode build design/ --output preview.png        # 렌더링 확인
ode pack design/                              # → design.ode
```

### 6.2 Figma 임포트 → 수정

```bash
ode import figma --token $TOKEN --file-key abc --output design/
# → assets/에 이미지 자동 저장
# → 에이전트가 document.json 직접 수정
ode pack design/                              # → design.ode
```

### 6.3 CLI vs 직접 편집

| 작업 | CLI | JSON 직접 편집 |
|------|:---:|:---:|
| 노드 추가/삭제/이동 | O | O |
| 스타일 변경 | O | O |
| 복잡한 구조 변경 (여러 노드 동시) | △ | O |
| 이미지 추가 | O | △ (에셋 파일 복사 + JSON에 Linked 참조 추가 필요) |
| 검증/렌더링 | O | X |

### 6.4 ZIP 모드 직접 작업

풀지 않고도 CLI로 직접 조작 가능:

```bash
ode add frame design.ode --name "Hero" --width 1920 --height 600
ode build design.ode --output preview.png
```
