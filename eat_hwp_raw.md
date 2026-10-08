# Hwp Ai Agent
LLM이 현재 HWP Document의 양식과 내용, 기초자료를 바탕으로 문서를 작성하거나
수정하는 HWP AI AGENT.

## Features

1. 현재 XHwpDocument의 양식(제목, 소제목, 글꼴, 문단모양)을 이해하고, 최대한 준수한다.
1. 내용을 작성할 때에는 문단의 직전, 직후 문단에 기재된 제목/소제목(문서 양식마다 1~4개의 레벨이 존재한다)에 상응하도록 내용을 작성한다.
1. 별도의 GUI를 통해 제공되는 파일(hwp 등, Retrieval-Augmented Generation)들에 기재된 사실/근거들을 바탕으로, 사용자가 원하는 내용을 제시한다.
1. 양식 준수의 기준 우선순위
    1. 줄 수
    1. 줄간격, 문단여백
    1. 자간
1. 사용자의 스타일을 학습하여 출력을 최적화하는 기능
   * 제목/소제목 문단을 수정/완성할 때에는 직후 문단을 참고하여 1줄 이내로 입력 또는 수정한다.
   * 별도 모드가 아니라 커서의 컨텍스트가 제목/소제목/본문이라는 정보를 프롬프트에 추가하는 기능
1. LLM Local or remote Server(nvidia spark dgx 등)를 지원
1. Claude api, Gemini api는 추후지원으로 일단 설계에만 반영

## Workflow
1. 사용자가 문단에 프롬프트 입력(생략가능) 
1. 사용자가 eat_hwp_raw 호출하는 addon의 `버튼1` 클릭 or 단축키 `Ctrl+]`
1. eat_hwp_raw GUI 팝업(별도 프로세스)
1. eat_hwp_raw는 hwp_automation_rs로 현재 컨텍스트(커서위치, 블록 정보 등) 확보
1. SaveAs로 TMP 디렉토리에 hwpx 저장 후 hwp사용자는 기존 문서 작업 계속, 또는 별개의 Workflow 실행 가능
1. eat_hwp_raw GUI에 자유롭게 프롬프트 입력 
1. eat_hwp_raw는 TMP에 저장된 hwpx를 [hwpers](https://github.com/Indosaram/hwpers)로
   파싱하고, 프롬프트 실행
1. 결과물은 별도의 새 hwp 창을 띄워서 거기에 입력하되, 그 결과물에 맞는 서식을
   최대한 적용
1. 처리완료 알람 팝업(windows api)

## eat_hwp_raw GUI
1. 현재문단/전체문서를 컨텍스트로 지정할 수 있는 check button
1. 프롬프트/답변 textarea(답변 textarea는 새 hwp에 창에 입력된 내용과 동일)
1. 프롬프트 commit 버튼
1. etc

## local llm server
1. 외부 llm service
1. local or intranet llm server(gemma4 or etc)
1. windows -> wsl2(localhost) 접속은 설정완료(ex: windows chrome browser에서
    wsl2에서 실행한 웹서버 localhost:8000 으로 접속가능)
1. vram 12GB, RTX5070

### 서버 구성 (확정: RTX 5070 12GB)
- **서빙: Ollama** — 포트 `11434`, OpenAI 호환 `/v1`. 단일 사용자 GUI엔 충분하고 GGUF
  양자화로 12GB에 적합. (대규모 동시성/원격 DGX로 확장 시 vLLM `:8000/v1`로 교체 — 앱은
  base URL env만 변경.)
- **모델: Gemma 4 12B dense, `Q4_K_M`** — 가중치 ~7–8GB로 12GB에 KV 캐시 여유. 26B A4B(MoE)는
  Q4 ~15GB로 12GB 초과(CPU 스필) → 제외. E4B는 양식 준수가 약해 폴백용.
- **컨텍스트: `num_ctx` 16K–32K** — 256K 풀은 KV 캐시가 VRAM을 초과. 문서+RAG를 커버하는 실용값.
- **드라이버/주의**: RTX 5070은 Blackwell(sm_120) → Windows에 최신 NVIDIA WSL 지원 드라이버
  (R570+) + 최신 Ollama 빌드 필수(구버전은 GPU 미인식). WSL 내부엔 Linux NVIDIA 드라이버 설치 금지.
- **`.wslconfig`**(`%UserProfile%\.wslconfig`): `[wsl2]` `memory=24GB`, `processors=<대부분>`,
  `swap=8GB`, `networkingMode=mirrored`(localhost 접속 보장, 이미 동작). vhdx 여유 60GB+.

#### 구동
```bash
# WSL2(Ubuntu 24.04) 안에서
curl -fsSL https://ollama.com/install.sh | sh        # 최신 = Blackwell 지원
ollama --version
ollama pull gemma4:12b                                 # 정확 태그는 `ollama list`로 확인
# (라이브러리에 없으면) HF GGUF를 import:
#   printf 'FROM ./gemma4-12b-Q4_K_M.gguf\nPARAMETER num_ctx 32768\n' > Modelfile
#   ollama create gemma4:12b -f Modelfile
ollama run gemma4:12b "한국어로 인사해줘"             # GPU 로드 + 한국어 확인
```

#### eat_hwp_raw 연결 (env, Windows 측)
- `EAT_HWP_LLM_BASE_URL` = `http://localhost:11434/v1` (= 기본값, mirrored라 그대로 닿음)
- `EAT_HWP_LLM_MODEL` = `gemma4:12b` (`ollama list` 태그와 정확히 일치)
- `EAT_HWP_LLM_API_KEY` = (빈 값, Ollama는 불필요)
