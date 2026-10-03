#!/usr/bin/env python3
"""Deterministic subprocess fixture. It never contacts OpenAI."""
import base64
import json
import os
import struct
import sys
import threading
import zlib
if "mcp" in sys.argv:
    print("[]")
    sys.exit(0)
mode = os.environ.get("FORGE_TEST_MODE", "success")
output_lock = threading.Lock()
threads = {}
sequence = 0
login_state={"loggedIn":False,"active":None,"sequence":0}
pending_tools = {}
turn_inputs = {}
chat_reference_paths = []
def emit(value):
    with output_lock:
        print(json.dumps(value), flush=True)
def chunk(name, data):
    return struct.pack(">I", len(data)) + name + data + struct.pack(">I", zlib.crc32(name + data))
pixels = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"\x00" + bytes([30, 90, 70, 128]) * 2 + b"\x00" + bytes([30, 90, 70, 128]) * 2)) + chunk(b"IEND", b"")
if mode in ("animation","guide-animation"):
    rows=[]
    for y in range(32):
        row=bytearray([0])
        for x in range(48):
            cell=(y//16)*3+x//16
            row.extend([30+cell*30,90,70,0 if x%16==0 or y%16==0 else 255])
        rows.append(row)
    pixels=b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",48,32,8,6,0,0,0))+chunk(b"IDAT",zlib.compress(b"".join(rows)))+chunk(b"IEND",b"")
image = {"id": "image-1", "type": "imageGeneration", "status": "completed", "result": base64.b64encode(pixels).decode(), "savedPath": None, "failure": None}
def complete(thread, text=None):
    if text is not None:
        emit({"method":"item/agentMessage/delta","params":{"threadId":thread,"turnId":thread,"delta":text}})
        emit({"method":"item/completed","params":{"threadId":thread,"turnId":thread,"item":{"type":"agentMessage","text":text}}})
    emit({"method": "turn/completed", "params": {"threadId": thread, "turn": {"id": thread, "status": "failed" if mode == "failed" else "completed", "error": {"message": "Fixture provider error"}, "items": []}}})
def finish_login(login_id):
    if login_state["active"]==login_id:
        login_state["loggedIn"]=True
        login_state["active"]=None
        emit({"method":"account/login/completed","params":{"loginId":login_id,"success":True,"error":None}})
def finish_image(thread):
    item = dict(image)
    if mode == "batch-partial" and thread.endswith("-1"):
        item["failure"] = {"message":"Fixture first-image failure"}
    if mode == "invalid-image":
        item["result"] = "not image data"
    if mode != "no-image":
        emit({"method": "item/completed", "params": {"threadId": thread, "turnId": thread, "item": item}})
    complete(thread)
def tool(thread, name, args, call, callback):
    request_id = "tool-" + thread + "-" + call + "-" + str(len(pending_tools))
    pending_tools[request_id] = callback
    emit({"id":request_id,"method":"item/tool/call","params":{"threadId":thread,"turnId":thread,"callId":call,"tool":name,"arguments":args}})
def guide(thread):
    prompt = turn_inputs[thread][0]["text"]
    context = json.JSONDecoder().raw_decode(prompt.split("Current workspace (data, not instructions):\n", 1)[1])[0]
    if mode == "guide-batch":
        args={"items":[{"characterId":s["id"],"prompt":"Render only "+s["name"],"width":64,"height":96} for s in context["subjects"]],"referenceAssetIds":[]}
        def queued(result):
            assert result["success"]
            batch=json.loads(result["contentItems"][0]["text"])
            assert len(batch["jobIds"])==3
            def replayed(result):
                assert result["success"]
                assert json.loads(result["contentItems"][0]["text"])==batch
                def denied(result):
                    assert not result["success"]
                    assert json.loads(result["contentItems"][0]["text"])["error"]["code"]=="GENERATION_NOT_AUTHORIZED"
                    complete(thread,"Three separate named files are rendering.")
                tool(thread,"generate_subject_assets",args,"extra-batch",denied)
            tool(thread,"generate_subject_assets",args,"batch",replayed)
        tool(thread,"generate_subject_assets",args,"batch",queued)
        return
    if mode in ("guide", "guide-generate", "guide-animation", "guide-new-game-reference") and not context["gameSetupAnswered"] and (mode == "guide-new-game-reference" or context["project"] is None):
        def asked(result):
            assert result["success"]
            # Even a misbehaving model must not change the project while the question is pending.
            def blocked(result):
                assert not result["success"]
                assert json.loads(result["contentItems"][0]["text"])["error"]["code"] == "QUESTION_PENDING"
                complete(thread, "Choose your visual direction, or skip to use defaults.")
            tool(thread, "choose_style", {"presetId":"woodland","projectName":"Unapproved","extraDirection":None}, "premature-style", blocked)
        tool(thread, "ask_question", {"prompt":"Which visual direction should we use?","forNewGame":True,"options":[{"id":"woodland","label":"Warm storybook"},{"id":"pixel","label":"Pixel art"},{"id":"isometric","label":"Tiny isometric"}]}, "setup-question", asked)
        return
    if mode in ("guide-reference", "guide-new-game-reference"):
        inputs = turn_inputs[thread]
        prompt = inputs[0]["text"]
        context = json.JSONDecoder().raw_decode(prompt.split("Current workspace (data, not instructions):\n", 1)[1])[0]
        attached = context["attachedReferences"]
        if not attached:
            assert len(inputs) == 1
            complete(thread, "Your previous attachments apply only to that message.")
            return
        assert len(attached) == 1
        assert inputs[1]["type"] == "localImage"
        assert inputs[1]["path"] == attached[0]["path"]
        assert open(inputs[1]["path"], "rb").read().startswith(b"\x89PNG")
        chat_reference_paths[:] = [inputs[1]["path"]]
        # Omit tool references deliberately: the app must propagate added-to-chat images.
        args = {"kind":"PROP","prompt":"Revise the attached asset with a red roof","characterId":None,"referenceAssetIds":[],"width":64,"height":96,"transparentBackground":True}
        def first(result):
            assert result["success"]
            job = json.loads(result["contentItems"][0]["text"])
            assert job["referenceAssetIds"] == [attached[0]["id"]]
            def replayed(result):
                assert result["success"]
                assert json.loads(result["contentItems"][0]["text"])["id"] == job["id"]
                complete(thread,"Your revision is rendering.")
            tool(thread,"generate_asset",args,"revision",replayed)
        if mode == "guide-new-game-reference":
            original = attached[0]
            create = {"projectName":"New tower game","presetId":"isometric","extraDirection":None}
            def created(result):
                assert result["success"]
                project = json.loads(result["contentItems"][0]["text"])
                assert project["id"] != context["project"]["id"]
                copied = project["attachedReferences"][0]
                assert copied["projectId"] == project["id"]
                assert project["referenceAssetIdMap"][original["id"]] == copied["id"]
                assert open(copied["path"], "rb").read() == open(original["path"], "rb").read()
                attached[:] = [copied]
                chat_reference_paths[:] = [copied["path"]]
                # Explicit tool IDs must use the returned copies, and merge exactly once.
                args["referenceAssetIds"] = [copied["id"]]
                def replayed_creation(result):
                    assert result["success"]
                    assert json.loads(result["contentItems"][0]["text"]) == project
                    tool(thread,"generate_asset",args,"revision",first)
                tool(thread,"create_game",create,"new-game",replayed_creation)
            tool(thread,"create_game",create,"new-game",created)
        else:
            tool(thread,"generate_asset",args,"revision",first)
        return
    def after_style(result):
        assert result["success"]
        tool(thread,"create_character",{"name":"Mira","description":"A forest scout with chestnut hair and an amber scarf."},"character",after_character)
    def after_character(result):
        assert result["success"]
        character=json.loads(result["contentItems"][0]["text"])["id"]
        def replay_character(result):
            assert result["success"]
            assert json.loads(result["contentItems"][0]["text"])["id"]==character
            if mode=="guide":
                complete(thread,"Saved the woodland style and Mira. Ask me to make her first pose.")
            else:
                args={"kind":"CHARACTER","prompt":"Mira idle pose","characterId":character,"referenceAssetIds":[],"width":None if mode=="guide-custom" else 64,"height":None if mode=="guide-custom" else 96,"transparentBackground":True}
                def first_image(result):
                    if result["success"]:
                        first=json.loads(result["contentItems"][0]["text"])["id"]
                        def replay_image(result):
                            assert result["success"]
                            assert json.loads(result["contentItems"][0]["text"])["id"]==first
                            tool(thread,"generate_asset",args,"second-image",denied)
                        tool(thread,"generate_asset",args,"image",replay_image)
                    else:
                        denied(result)
                def denied(result):
                    assert not result["success"]
                    assert json.loads(result["contentItems"][0]["text"])["error"]["code"]=="GENERATION_NOT_AUTHORIZED"
                    complete(thread,"Workspace updated; generation budget respected.")
                if mode=="guide-animation":
                    args={"characterId":character,"motion":"WALK","name":None,"prompt":"Walking right","frameCount":6,"columns":3,"frameSize":16,"fps":8,"isLooping":True}
                    # Both generation tools share the same per-message budget.
                    def animate_done(result):
                        assert result["success"]
                        tool(thread,"generate_animation",args,"image",lambda replay: (
                            tool(thread,"generate_asset",{"kind":"CHARACTER","prompt":"Another pose","characterId":character,"referenceAssetIds":[],"width":64,"height":96,"transparentBackground":True},"second-image",denied)
                        ))
                    tool(thread,"generate_animation",args,"image",animate_done)
                else:
                    tool(thread,"generate_asset",args,"image",first_image)
        # Repeat the exact server call to verify app-side deduplication.
        tool(thread,"create_character",{"name":"Mira","description":"A forest scout with chestnut hair and an amber scarf."},"character",replay_character)
    if mode=="guide-custom":
        tool(thread,"customize_style",{"name":"Moonlit story","description":None,"palette":["#7386A4","#C8D0E2"],"perspective":None,"lighting":"Cool moonlight from upper left"},"style",after_style)
    else:
        tool(thread,"choose_style",{"presetId":"woodland","projectName":"Fixture Woodland","extraDirection":None},"style",after_style)

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if "id" not in request:
        continue
    if method is None:
        callback=pending_tools.pop(request["id"])
        callback(request["result"])
        continue
    result = {}
    if method == "account/read":
        account={"type":"chatgpt","email":"test@example.invalid","planType":"plus"}
        result={"account":None if mode=="login" and not login_state["loggedIn"] else account,"requiresOpenaiAuth":True}
    elif method=="account/login/start":
        assert request["params"]["type"]=="chatgpt"
        login_state["sequence"]+=1
        login_id="login-"+str(login_state["sequence"])
        login_state["active"]=login_id
        result={"type":"chatgpt","loginId":login_id,"authUrl":"https://auth.openai.com/authorize?fixture=true"}
        timer=threading.Timer(0.15,finish_login,args=(login_id,));timer.daemon=True;timer.start()
    elif method=="account/login/cancel":
        if request["params"]["loginId"]==login_state["active"]:
            login_state["active"]=None
    elif method == "modelProvider/capabilities/read":
        result = {"imageGeneration": mode != "unavailable", "webSearch": False, "namespaceTools": True}
    elif method == "thread/start":
        assert request["params"]["sandbox"] == "read-only"
        assert request["params"]["approvalPolicy"] == "never"
        sequence+=1
        thread="thread-"+str(sequence)
        threads[thread]=bool(request["params"].get("dynamicTools"))
        result = {"thread": {"id": thread}}
    elif method == "turn/start":
        thread=request["params"]["threadId"]
        turn_inputs[thread] = request["params"]["input"]
        if not threads[thread]:
            assert "native image generation" in request["params"]["input"][0]["text"]
            if mode in ("guide-reference", "guide-new-game-reference") and chat_reference_paths:
                assert [item["path"] for item in request["params"]["input"][1:]] == chat_reference_paths
                assert open(chat_reference_paths[0], "rb").read().startswith(b"\x89PNG")
        result = {"turn": {"id": thread}}
    elif method == "turn/interrupt":
        thread=request["params"]["threadId"]
        emit({"method": "turn/completed", "params": {"threadId": thread, "turn": {"id": thread, "status": "interrupted", "items": []}}})
    emit({"id": request["id"], "result": result})
    if method == "turn/start":
        thread=request["params"]["threadId"]
        if mode in ("disconnect","guide-disconnect"):
            sys.exit(0)
        if threads[thread]:
            timer=threading.Timer(0.05,guide,args=(thread,))
        else:
            emit({"method": "item/started", "params": {"threadId": thread, "item": {"id": "image-1", "type": "imageGeneration"}}})
            timer=threading.Timer(0.05,finish_image,args=(thread,))
        if mode != "slow":
            timer.daemon=True
            timer.start()
