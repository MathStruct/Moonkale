// Geometry only. Rust supplies source identity and grapheme boundaries.
const request = await dioxus.recv();
await new Promise(resolve => requestAnimationFrame(resolve));
const node = document.getElementById(request.id);
if (request.replacements?.length) {
    const scalars = Array.from(request.text);
    const children = node ? [...node.children] : [];
    let offset = 0;
    const valid = node && children.every(child => {
        const start = Number(child.dataset.sourceStart), end = Number(child.dataset.sourceEnd);
        const replacement = request.replacements.find(value => value.start === start && value.end === end);
        const expected = replacement ? replacement.widget ?? '' : scalars.slice(start, end).join('');
        const matches = start === offset && end > start && end <= scalars.length && child.textContent === expected;
        offset = end;
        return matches;
    }) && offset === scalars.length;
    if (!valid) {
        dioxus.send(null);
    } else {
        const origin = node.getBoundingClientRect();
        const boxes = [];
        const measured = new Set();
        for (const segment of request.segments) {
            const replacement = request.replacements.find(value => value.start <= segment.start && segment.end <= value.end);
            let rect;
            let start = segment.start, end = segment.end;
            if (replacement) {
                if (measured.has(replacement.start)) continue;
                measured.add(replacement.start);
                start = replacement.start; end = replacement.end;
                const child = children.find(child => Number(child.dataset.sourceStart) === start);
                const range = document.createRange(); range.selectNodeContents(child);
                rect = replacement.widget ? range.getBoundingClientRect() : child.getBoundingClientRect();
                if (!replacement.widget) {
                    const neighbor = children.find(child => Number(child.dataset.sourceStart) === end && child.textContent)
                        ?? children.findLast(child => Number(child.dataset.sourceEnd) === start && child.textContent);
                    const range = document.createRange();
                    if (neighbor) range.selectNodeContents(neighbor);
                    const adjacent = neighbor ? range.getBoundingClientRect() : origin;
                    rect = {x:rect.x, y:adjacent.y, width:0, height:adjacent.height};
                }
            } else {
                const first = children.find(child => Number(child.dataset.sourceStart) === start);
                const last = children.find(child => Number(child.dataset.sourceEnd) === end);
                if (!first?.firstChild || !last?.lastChild) continue;
                const range = document.createRange(); range.setStart(first.firstChild,0);
                range.setEnd(last.lastChild,last.lastChild.textContent.length);
                rect = range.getBoundingClientRect();
            }
            boxes.push({start,end,x:rect.x-origin.x,y:rect.y-origin.y,width:rect.width,height:rect.height});
        }
        dioxus.send({boxes,width:origin.width,height:origin.height});
    }
    await dioxus.recv();
} else {
const nodes = [];
let total = 0;
if (node) {
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
        const text = walker.currentNode;
        nodes.push({node:text, start:total, end:total + text.length});
        total += text.length;
    }
}
if (!node || node.textContent !== request.text || total !== request.text.length) {
    if (node) node.dataset.geometryResult = `source-mismatch:${total}:${request.text.length}`;
    dioxus.send(null);
} else {
    const origin = node.getBoundingClientRect();
    const boundary = (offset, ending = false) => {
        const entry = nodes.find(entry => offset < entry.end || ending && offset === entry.end && offset > entry.start) ?? nodes.at(-1);
        return entry ? [entry.node, offset - entry.start] : null;
    };
    const boxes = [];
    for (const segment of request.segments) {
        const start = boundary(segment.utf16_start), end = boundary(segment.utf16_end, true);
        if (!start || !end) continue;
        const range = document.createRange();
        range.setStart(...start); range.setEnd(...end);
        for (const rect of range.getClientRects()) {
            boxes.push({start:segment.start, end:segment.end, x:rect.x-origin.x,
                y:rect.y-origin.y, width:rect.width, height:rect.height});
        }
    }
    const invalid = boxes.find(rect => rect.height <= 0 || rect.width < 0 ||
        ![rect.x,rect.y,rect.width,rect.height].every(Number.isFinite));
    node.dataset.geometryResult = invalid ? `invalid-box:${JSON.stringify(invalid)}` : 'measured';
    dioxus.send({boxes, width:origin.width, height:origin.height});
}
// Keep the desktop evaluator alive until Rust has consumed the reply. Closing
// immediately can let the native channel's GC remove a queued measurement.
await dioxus.recv();
}
