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
1. 프롬프트/답변 textarea
1. 프롬프트 commit 버튼
1. etc
