// Native WebKit DOM/layout probe, not OS input or IME acceptance.
const wait = async predicate => {
    for (let i = 0; i < 100; i++) {
        if (predicate()) return;
        await new Promise(resolve => setTimeout(resolve, 100));
    }
    throw Error("proportional native probe timed out: " + predicate.toString() + " " + JSON.stringify({fonts:document.fonts?.status,runs:[...document.querySelectorAll(".primary .mk-proportional-run")].map(node=>({...node.dataset}))}));
};
try {
    await wait(() => document.getElementById("show-proportional"));
    document.getElementById("layout-navigation-document").click();
    await wait(() => document.querySelector(".canonical")?.textContent.startsWith("Wi 😀"));
    const canonical = document.querySelector(".canonical").textContent;
    document.getElementById("layout-proportional").click();
    await wait(() => { const runs = [...document.querySelectorAll(".primary .mk-proportional-run")]; return runs.length > 1 && runs.every(run => run.dataset.ready === "true"); });
    const markedSource = document.querySelector(".primary .mk-proportional-text");
    if ([...document.querySelectorAll(".primary .mk-proportional-text")].map(el => el.textContent).join("") !== canonical.split("\n")[0]) throw Error("native marked source mismatch");
    const firstRect = markedSource.firstElementChild.getBoundingClientRect();
    markedSource.dispatchEvent(new MouseEvent("mousedown", {bubbles:true, button:0, buttons:1, clientX:firstRect.x + firstRect.width * .2, clientY:firstRect.y + firstRect.height / 2}));
    markedSource.dispatchEvent(new MouseEvent("mouseup", {bubbles:true, button:0}));
    const sharedInput = document.querySelector(".primary .mk-editor-core-input-sink");
    let edge = 0;
    for (const span of markedSource.children) {
        if (span.getBoundingClientRect().y > firstRect.y + .5) break;
        edge += [...span.textContent].length;
    }
    sharedInput.dispatchEvent(new KeyboardEvent("keydown", {bubbles:true,key:"End",code:"End"}));
    await wait(() => Number(document.querySelector(".primary .mk-native-surface").dataset.cursorOffset) === edge);
    const caret = () => document.querySelector(".primary .mk-proportional-run .mk-editor-core-caret")?.getBoundingClientRect();
    await wait(() => caret() && Math.abs(caret().y-firstRect.y)<2);
    sharedInput.dispatchEvent(new KeyboardEvent("keydown", {bubbles:true,key:"ArrowRight",code:"ArrowRight"}));
    await wait(() => caret()?.y > firstRect.y + 10);
    if (Number(document.querySelector(".primary .mk-native-surface").dataset.cursorOffset) !== edge) throw Error("native Right skipped wrap source boundary");
    sharedInput.dispatchEvent(new KeyboardEvent("keydown", {bubbles:true,key:"ArrowLeft",code:"ArrowLeft"}));
    await wait(() => caret() && Math.abs(caret().y-firstRect.y)<2);
    sharedInput.dispatchEvent(new KeyboardEvent("keydown", {bubbles:true,key:"Home",code:"Home"}));
    await wait(() => document.querySelector(".primary .mk-native-surface").dataset.cursorOffset === "0");
    sharedInput.dispatchEvent(new KeyboardEvent("keydown", {bubbles:true,key:"ArrowDown",code:"ArrowDown"}));
    await wait(() => Number(document.querySelector(".primary .mk-native-surface").dataset.cursorOffset) > 0);
    sharedInput.dispatchEvent(new KeyboardEvent("keydown", {bubbles:true,key:"ArrowUp",code:"ArrowUp"}));
    await wait(() => document.querySelector(".primary .mk-native-surface").dataset.cursorOffset === "0");
    const previousAdvance=markedSource.firstElementChild.getBoundingClientRect().width;
    const previousEpoch=Number(document.querySelector(".primary .mk-proportional-run").dataset.fontEpoch);
    const face=new FontFace("NativeGeometryAcceptance", 'local("DejaVu Sans Mono"), local("Liberation Mono"), local("Noto Sans Mono")');
    document.fonts.add(face);
    const fontStyle=document.createElement("style");
    fontStyle.textContent=".mk-proportional-text {font-family:NativeGeometryAcceptance,serif !important}";
    document.head.append(fontStyle);
    await face.load();
    await wait(() => { const runs=[...document.querySelectorAll(".primary .mk-proportional-run")]; return runs.every(run => run.dataset.ready === "true" && Number(run.dataset.fontEpoch)>previousEpoch); });
    if (document.querySelector(".primary .mk-native-surface").dataset.cursorOffset !== "0") throw Error("font load moved native source caret");
    const changedAdvance=markedSource.firstElementChild.getBoundingClientRect().width;
    if (Math.abs(changedAdvance-previousAdvance)<.5) throw Error("native font did not change glyph advances");
    const changedRect=markedSource.children[1].getBoundingClientRect();
    markedSource.dispatchEvent(new MouseEvent("mousedown", {bubbles:true,button:0,buttons:1,clientX:changedRect.x+changedRect.width*.55,clientY:changedRect.y+changedRect.height/2}));
    markedSource.dispatchEvent(new MouseEvent("mouseup", {bubbles:true,button:0}));
    await wait(() => document.querySelector(".primary .mk-native-surface").dataset.cursorOffset === "2");
    const resetRect=markedSource.firstElementChild.getBoundingClientRect();
    markedSource.dispatchEvent(new MouseEvent("mousedown", {bubbles:true,button:0,buttons:1,clientX:resetRect.x+resetRect.width*.2,clientY:resetRect.y+resetRect.height/2}));
    markedSource.dispatchEvent(new MouseEvent("mouseup", {bubbles:true,button:0}));
    await wait(() => document.querySelector(".primary .mk-native-surface").dataset.cursorOffset === "0");
    sharedInput.value = "Q";
    sharedInput.dispatchEvent(new Event("input", {bubbles:true}));
    await wait(() => document.querySelector(".canonical")?.textContent === "Q" + canonical);
    document.getElementById("layout-uniform").click();
    document.getElementById("show-proportional").click();
    const probe = () => document.querySelector(".proportional-probe");
    await wait(() => probe()?.dataset.ready === "true");
    const before = Number(probe().dataset.height);
    document.getElementById("proportional-width").click();
    await wait(() => probe()?.dataset.ready === "true" && Number(probe().dataset.width) === 230);
    if (Number(probe().dataset.height) <= before) throw Error("native proportional text did not wrap by pixels");
    document.getElementById("proportional-reset").click();
    await wait(() => probe()?.dataset.ready === "true" && document.querySelector(".proportional-canonical")?.textContent === "Wi 😀 é 中\tTabs");
    const source = document.querySelector(".proportional-source");
    const node = [...source.childNodes].find(node => node.nodeType === Node.TEXT_NODE);
    const range = document.createRange();
    range.setStart(node, 3); range.setEnd(node, 5);
    const rect = range.getBoundingClientRect();
    source.dispatchEvent(new MouseEvent("mousedown", {bubbles:true, button:0, buttons:1, clientX:rect.x + rect.width * .2, clientY:rect.y + rect.height / 2}));
    source.dispatchEvent(new MouseEvent("mouseup", {bubbles:true, button:0}));
    await wait(() => probe().dataset.cursor === "3");
    const input = document.querySelector(".proportional-input");
    input.value = "Q";
    input.dispatchEvent(new Event("input", {bubbles:true}));
    await wait(() => document.querySelector(".proportional-canonical")?.textContent === "Wi Q😀 é 中\tTabs");
    dioxus.send({ok:true, width:Number(probe().dataset.width), note:"Native WebKit Range geometry and synthetic DOM input; OS/IME acceptance remains open."});
} catch (error) {
    dioxus.send({ok:false, error:String(error)});
}
