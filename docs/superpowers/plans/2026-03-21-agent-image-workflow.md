# Agent Image Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ODE 이미지 라이프사이클을 end-to-end 검증하고, 발견된 갭을 수정하며, 에이전트용 이미지 워크플로우 가이드를 작성한다.

**Architecture:** 기존 이미지 파이프라인(ImageSource → AssetStore → convert.rs → render.rs → export)이 이미 구현되어 있다. `cmd_build`는 `preload_all()`을 렌더링 전에 호출한다. 외부 경로 이미지는 `std::fs::read()`로 직접 로드, assets/ 경로는 `get_loaded()`로 프리로드된 데이터 사용. 감사 → 테스트 → 수정 → 문서화 순서로 진행.

**Tech Stack:** Rust, tiny-skia (래스터 렌더링), image crate (디코딩), serde_json (document.json)

**Spec:** `docs/superpowers/specs/2026-03-21-agent-image-workflow-design.md`

---

### Task 0: 이미지 라이프사이클 코드 감사 (Phase 1)

스펙의 Phase 1에 해당. 이미지가 ODE에서 거치는 4단계를 코드 레벨에서 감사하고 발견 사항을 기록한다.

**Files:**
- Read: `crates/ode-cli/src/mutate.rs:467-506` (cmd_add image 분기)
- Read: `crates/ode-format/src/asset.rs` (AssetStore 전체)
- Read: `crates/ode-format/src/container.rs:117-151` (open_unpacked), `:182-245` (open_packed), `:303-359` (save_packed), `:365-386` (extract_embedded_assets)
- Read: `crates/ode-core/src/convert.rs:327-376` (emit_image)
- Read: `crates/ode-core/src/render.rs:152-180` (DrawImage)
- Read: `crates/ode-export/src/svg.rs:171-193` (SVG image export)
- Read: `crates/ode-export/src/pdf.rs:153-170` (PDF image export)

- [ ] **Step 1: 입력(Input) 감사**

`crates/ode-cli/src/mutate.rs`의 `cmd_add()` image 분기를 읽는다.

확인 사항:
- `--src` 값이 `ImageSource::Linked { path }` 로 그대로 저장되는지 (파일 복사 없이 경로만)
- 경로 유효성 검증 유무
- 지원 포맷 제한 유무

결과를 아래 형식으로 기록:
```
## 감사 결과: 입력
- 동작: [실제 동작 설명]
- 갭: [발견된 갭]
- 수정 필요: [Y/N + 이유]
```

- [ ] **Step 2: 저장(Storage) 감사**

`crates/ode-format/src/asset.rs` 전체와 `container.rs`의 open/save 로직을 읽는다.

확인 사항:
- `AssetStore.compute_hash()`: SHA-256 해시 길이 (16 hex chars = 8 bytes)
- `register_on_disk()` vs `add_image_with_hash()` 차이
- `preload_all()`: OnDisk → Loaded 변환 로직
- `save_packed()`: Linked 이미지가 ZIP에 포함되는 경로
- `extract_embedded_assets()`: Embedded → Linked 변환 로직

- [ ] **Step 3: 렌더링(Rendering) 감사**

`crates/ode-core/src/convert.rs`의 `emit_image()`와 `render.rs`의 DrawImage 처리를 읽는다.

확인 사항:
- 이미지 데이터 로딩 분기: `assets/` 접두사 → `get_loaded()`, 기타 → `std::fs::read()`
- 에러 핸들링: 실패 시 silent return (로깅 없음) 여부
- `ImageFillMode` 연결 상태: 데이터 모델에만 존재하고 렌더링에 미연결인지 확인
- 이미지 디코딩: `image::load_from_memory()` 사용, 지원 포맷 확인

- [ ] **Step 4: 내보내기(Export) 감사**

`crates/ode-export/src/svg.rs`과 `pdf.rs`의 이미지 처리를 읽는다.

확인 사항:
- SVG: base64 data URI로 인라인 임베딩 (`image/png` 또는 `image/jpeg` MIME)
- PDF: `image` crate로 디코딩 → krilla Image 객체로 변환

- [ ] **Step 5: 감사 결과 정리 및 커밋**

4단계 감사 결과를 `/tmp/image-audit-results.md`에 정리한다. 포함 사항:
- 각 단계별 동작 요약
- 발견된 갭 목록 (수정 필요 vs 후속 과업)
- 이후 테스트에서 집중할 리스크 영역

```bash
# 코드 수정이 있었다면만 커밋. 감사 자체는 읽기 전용이므로 커밋 불필요할 수 있음.
```

---

### Task 1: 외부 경로 Linked 이미지 렌더링 검증 (T1)

에이전트가 document.json에 `ImageSource::Linked { path }` 를 외부 파일 절대 경로로 작성하고 `ode build`로 렌더링되는지 검증한다.

**Files:**
- Read: `crates/ode-core/src/convert.rs:327-376` (emit_image 함수)
- Read: `crates/ode-cli/src/commands.rs:199-250` (cmd_build 함수)

- [ ] **Step 1: 테스트용 이미지 생성**

100x100 빨간색 PNG 이미지를 생성한다:

```bash
python3 -c "
from PIL import Image
img = Image.new('RGB', (100, 100), (255, 0, 0))
img.save('/tmp/test-red.png')
"
```

Pillow 없으면:
```bash
convert -size 100x100 xc:red /tmp/test-red.png
```

둘 다 안 되면 인터넷에서 아무 PNG 다운로드 또는 프로젝트 내 기존 이미지 사용.

검증: `ls -la /tmp/test-red.png` → 파일 존재, 크기 > 0

- [ ] **Step 2: 이미지 노드가 포함된 document.json 작성**

```bash
./target/release/ode new --name "image-test" /tmp/image-test/
```

`/tmp/image-test/document.json`을 아래 내용으로 교체:

```json
{
  "format_version": [0, 2, 0],
  "name": "Image Test",
  "canvas": ["root"],
  "nodes": [
    {
      "stable_id": "root",
      "name": "Root",
      "type": "frame",
      "width": 300,
      "height": 300,
      "clips_content": true,
      "visual": {
        "fills": [{
          "paint": { "type": "solid", "color": { "space": "srgb", "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } },
          "opacity": 1.0, "blend_mode": "normal", "visible": true
        }]
      },
      "container": { "children": ["img1"] }
    },
    {
      "stable_id": "img1",
      "name": "Test Image",
      "transform": { "a": 1, "b": 0, "c": 0, "d": 1, "tx": 50, "ty": 50 },
      "type": "image",
      "source": { "type": "linked", "path": "/tmp/test-red.png" },
      "width": 200,
      "height": 200
    }
  ],
  "tokens": { "collections": [], "active_modes": {} },
  "working_color_space": "srgb"
}
```

핵심: `"path": "/tmp/test-red.png"` — **절대 경로** 사용.

- [ ] **Step 3: ode build 실행 및 결과 확인**

```bash
./target/release/ode build /tmp/image-test/ --output /tmp/image-test/output.png
```

Expected stdout: `{"status":"ok","path":"/tmp/image-test/output.png","width":300,"height":300}`

검증:
1. Read 도구로 `/tmp/image-test/output.png` 열어서 빨간 사각형이 흰 배경 위에 보이는지 시각 확인
2. 파일 크기 확인: `ls -la /tmp/image-test/output.png` — 빈 캔버스(~수 KB)보다 큰지 확인

- [ ] **Step 4: 실패 시 디버깅 및 수정**

이미지가 안 보이면 아래 순서로 조사:
1. `emit_image()` (`crates/ode-core/src/convert.rs:327-376`)에서 외부 경로 분기(`std::fs::read()`) 진입 여부 확인 — 필요 시 `eprintln!` 로그 추가
2. 경로가 CWD 기준으로 해석되는지, 절대 경로가 그대로 전달되는지 확인
3. `image::load_from_memory()` 디코딩 실패 여부 확인

수정 범위: `crates/ode-core/src/convert.rs` 또는 `crates/ode-core/src/render.rs`만 수정. 수정 후 Step 3 재실행으로 검증.

- [ ] **Step 5: SVG/PDF 출력도 검증**

```bash
./target/release/ode build /tmp/image-test/ --output /tmp/image-test/output.svg
./target/release/ode build /tmp/image-test/ --output /tmp/image-test/output.pdf
```

검증: SVG 파일에 `<image ... href="data:image/png;base64,..."` 존재 확인. PDF 파일 크기 > 빈 문서 크기.

- [ ] **Step 6: 커밋 (수정이 있었다면)**

코드 수정이 있었다면:
```bash
git add crates/ode-core/src/convert.rs crates/ode-core/src/render.rs
git commit -m "fix: resolve image rendering for linked external paths"
```

수정 없이 통과 → 커밋 불필요, 다음 Task 이동.

---

### Task 2: Assets 디렉토리 + Pack/Unpack 워크플로우 검증 (T2)

에이전트가 이미지를 `assets/`에 해시 파일명으로 복사하고, document.json에서 `assets/{hash}.{ext}` 경로로 참조한 뒤, `ode pack` → `ode build`가 작동하는지 검증한다.

**Files:**
- Read: `crates/ode-format/src/asset.rs` (compute_hash — SHA-256 해시 16 hex chars)
- Read: `crates/ode-format/src/container.rs:117-151` (open unpacked — asset scan)

- [ ] **Step 1: 이미지 해시 계산 및 assets/에 복사**

```bash
mkdir -p /tmp/image-test/assets
HASH=$(python3 -c "import hashlib; print(hashlib.sha256(open('/tmp/test-red.png','rb').read()).hexdigest()[:16])")
cp /tmp/test-red.png "/tmp/image-test/assets/${HASH}.png"
echo "Hash: $HASH"
```

검증: `ls /tmp/image-test/assets/` → `{16자리해시}.png` 파일 존재.

참고: `asset.rs`의 `compute_hash()`가 SHA-256의 앞 16 hex chars를 사용하는지 코드로 확인. 불일치 시 해시 길이 조정.

- [ ] **Step 2: document.json에서 assets/ 경로로 참조**

`/tmp/image-test/document.json`의 이미지 노드 source를 수정:
```json
"source": { "type": "linked", "path": "assets/{실제HASH}.png" }
```

- [ ] **Step 3: 언팩 상태에서 빌드 및 검증**

```bash
./target/release/ode build /tmp/image-test/ --output /tmp/image-test/output-unpacked.png
```

검증: Read 도구로 열어서 이미지 보이는지 확인. Task 1 결과와 동일해야 함.

- [ ] **Step 4: Pack 후 packed 상태에서 빌드**

```bash
./target/release/ode pack /tmp/image-test/
./target/release/ode build /tmp/image-test.ode --output /tmp/image-test/output-packed.png
```

검증: `output-packed.png`에도 이미지 보이는지 확인.

- [ ] **Step 5: 실패 시 디버깅 및 수정**

문제 발생 시 확인 순서:
1. `container.rs` open_unpacked의 asset 스캔: 파일명 stem → hash 추출 로직이 실제 해시 형식과 일치하는지
2. `preload_all()`: `OnDisk` → `Loaded` 변환 정상 작동 확인
3. `emit_image()`: `get_loaded(hash)` 호출 시 hash 키 매칭 확인

수정 범위: `crates/ode-format/src/asset.rs` 또는 `crates/ode-format/src/container.rs`. 수정 후 Step 3-4 재실행.

- [ ] **Step 6: 커밋 (수정이 있었다면)**

```bash
git add crates/ode-format/src/asset.rs crates/ode-format/src/container.rs
git commit -m "fix: asset pipeline for packed/unpacked image workflow"
```

---

### Task 3: 이미지 위치/크기 변경 검증 (T3)

이미지 노드의 위치와 크기를 CLI로 변경했을 때 렌더링에 올바르게 반영되는지 확인한다.

**Files:**
- Read: `crates/ode-core/src/render.rs:152-180` (DrawImage 스케일링: sx = width/img_w, sy = height/img_h)

- [ ] **Step 1: 전체 크기로 변경 후 빌드**

Task 1의 document.json을 사용 (외부 경로 Linked 상태):
```bash
./target/release/ode set /tmp/image-test/ img1 --x 0 --y 0 --width 300 --height 300
./target/release/ode build /tmp/image-test/ --output /tmp/image-test/output-fullsize.png
```

검증: 이미지가 프레임 전체(300x300)를 채우는지 확인.

- [ ] **Step 2: 작은 크기 + 다른 위치로 변경 후 빌드**

```bash
./target/release/ode set /tmp/image-test/ img1 --x 100 --y 100 --width 50 --height 50
./target/release/ode build /tmp/image-test/ --output /tmp/image-test/output-small.png
```

검증: 이미지가 (100,100) 위치에 50x50 크기로 보이는지 확인. 주변은 흰 배경.

- [ ] **Step 3: 커밋 (수정이 있었다면)**

```bash
git add crates/ode-core/src/render.rs
git commit -m "fix: image position/size rendering corrections"
```

---

### Task 4: 기존 포스터에 이미지 삽입 (최종 검증)

Task 1-3 검증 완료 후, 실제 포스터 document.json에 이미지를 추가하여 최종 검증한다.

**배경:** `/tmp/poster/document.json`에 39노드 포스터가 존재한다. `card` 프레임(stable_id: "card")이 28개 자식 노드를 가지며, accent-bar부터 footer까지 포함. 이 card에 이미지 노드를 추가한다.

**Files:**
- Modify: `/tmp/poster/document.json`

- [ ] **Step 1: 포스터의 card children 배열에 이미지 ID 추가**

`/tmp/poster/document.json`의 `card` 노드 → `container.children` 배열 맨 앞에 `"logo-img"` 추가.

- [ ] **Step 2: 이미지 노드 추가**

nodes 배열에 추가:
```json
{
  "stable_id": "logo-img",
  "name": "Logo Image",
  "transform": { "a": 1, "b": 0, "c": 0, "d": 1, "tx": 50, "ty": 15 },
  "type": "image",
  "source": { "type": "linked", "path": "/tmp/test-red.png" },
  "width": 100,
  "height": 25
}
```

- [ ] **Step 3: 포스터 빌드 및 최종 검증**

```bash
./target/release/ode build /tmp/poster/ --output /tmp/poster/output-with-image.png
```

검증 조건 (스펙 Phase 2 최종 검증 기준):
1. 출력 PNG에 이미지가 시각적으로 보임 (Read 도구로 확인)
2. 이미지 위치가 transform 좌표(tx=50, ty=15)와 일치
3. 기존 포스터 요소(텍스트, 프레임, 배지 등)가 깨지지 않음

---

### Task 5: 에이전트 이미지 워크플로우 가이드 작성

검증 결과를 바탕으로 에이전트가 바로 따라할 수 있는 실용 가이드를 작성한다.

**Files:**
- Create: `design-knowledge/guides/agent-image-workflow.md`

- [ ] **Step 1: 가이드 문서 작성**

Task 0-4 결과를 바탕으로 작성. 구조:

```markdown
# 에이전트 이미지 워크플로우 가이드

## 이미지 라이프사이클 개요
- 입력 → 저장 → 렌더링 → 내보내기 흐름
- Linked vs Embedded 차이와 선택 기준

## 워크플로우 레시피

### 레시피 A: document.json 직접 작성 (권장)
[Task 1에서 검증된 이미지 노드 JSON 스니펫]

### 레시피 B: CLI로 이미지 추가
[ode add image --src 사용법 + 제약사항]

### 레시피 C: 언팩 → 이미지 복사 → 팩
[Task 2에서 검증된 self-contained .ode 워크플로우]

## 주의사항 / 알려진 제약
- 지원 포맷 (Task 0 감사 기반)
- ImageFillMode: 데이터 모델에 존재하나 렌더링 미연결 (후속 과업)
- CLI로 설정 불가한 속성: [목록], document.json 직접 편집 필요
- 경로 해석: [CWD vs document 기준, Task 0에서 확인된 동작]
- 에러 핸들링: 이미지 미존재 시 silent failure (경고 없음)

## 코드 수정 이력
[Task 1-4에서 수정한 내용과 이유]
[수정 없이 통과한 경우 "수정 없음 — 기존 파이프라인 정상 작동" 기록]
```

내용은 Task 0-4 실행 결과의 **실제 데이터**로 채운다.

- [ ] **Step 2: 가이드의 레시피 A를 새 문서에서 재현하여 검증**

가이드만 보고 처음부터 이미지를 삽입할 수 있는지 확인. 빠진 단계가 있으면 가이드에 추가.

- [ ] **Step 3: 발견된 갭 중 후속 과업 기록**

수정하지 않은 갭(ImageFillMode 미연결, silent failure 등)을 가이드의 "알려진 제약" 섹션에 명시. 후속 스펙이 필요한 항목은 목록으로 정리.

- [ ] **Step 4: 커밋**

```bash
git add design-knowledge/guides/agent-image-workflow.md
git commit -m "docs: add agent image workflow guide

Practical guide for AI agents to insert and render images in ODE
documents. Covers three workflows: direct JSON, CLI, and pack/unpack.
Based on end-to-end verification of the image pipeline."
```

---

### Task 6: 스펙 문서 상태 업데이트

- [ ] **Step 1: 스펙 Status 변경**

`docs/superpowers/specs/2026-03-21-agent-image-workflow-design.md`의 `**Status:** Draft`를 `**Status:** Completed`로 변경.

- [ ] **Step 2: 커밋**

```bash
git add docs/superpowers/specs/2026-03-21-agent-image-workflow-design.md
git commit -m "docs: mark agent image workflow spec as completed"
```
