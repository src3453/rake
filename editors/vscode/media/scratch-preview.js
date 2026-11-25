(function () {
  const vscode = acquireVsCodeApi();
  const canvas = document.getElementById("canvas");
  const count = document.getElementById("count");
  const status = document.getElementById("status");
  const dot = document.getElementById("dot");
  const documentName = document.getElementById("document-name");
  const palette = {
    event: "category-event", data: "category-data", control: "category-control",
    operator: "category-operator", sensing: "category-sensing", motion: "category-motion",
    looks: "category-looks", sound: "category-sound", procedures: "category-procedures",
    default: "category-default"
  };

  function setStatus(message, kind) {
    status.textContent = message;
    dot.className = `dot${kind ? ` ${kind}` : ""}`;
  }
  function category(opcode) {
    return palette[opcode.split("_")[0]] || palette.default;
  }
  function field(block, name) {
    const value = block.fields && block.fields[name];
    return Array.isArray(value) ? String(value[0]) : "";
  }
  function input(block, name) {
    return block.inputs && block.inputs[name];
  }
  function inputBlockId(value) {
    return Array.isArray(value) && typeof value[1] === "string" ? value[1] : null;
  }
  function mutationArray(mutation, key) {
    try {
      const value = JSON.parse(mutation && mutation[key] || "[]");
      return Array.isArray(value) ? value : [];
    } catch (_) {
      return [];
    }
  }
  function procedureParts(mutation, asDefinition) {
    const ids = mutationArray(mutation, "argumentids");
    const names = mutationArray(mutation, "argumentnames");
    const procedureName = String(mutation && mutation.proccode || "custom block").split(" %")[0];
    const result = [textPart(`${procedureName}(`)];
    ids.forEach((id, index) => {
      if (index) result.push(textPart(", "));
      result.push(asDefinition ? textPart(String(names[index] || id)) : inputPart(id));
    });
    result.push(textPart(")"));
    return result;
  }
  function shadowText(value) {
    if (!Array.isArray(value)) return "";
    const item = value[2] || value[1];
    if (!Array.isArray(item)) return "";
    return String(item[1] ?? "");
  }
  const textPart = value => ({ kind: "text", value });
  const fieldPart = value => ({ kind: "field", value });
  const inputPart = value => ({ kind: "input", value });

  function parts(block, blocks) {
    const t = textPart, f = fieldPart, i = inputPart;
    if (block.opcode === "procedures_call") return procedureParts(block.mutation, false);
    if (block.opcode === "procedures_definition") {
      const prototypeId = inputBlockId(input(block, "custom_block"));
      const prototype = prototypeId && blocks[prototypeId];
      return procedureParts(prototype && prototype.mutation, true);
    }
    const table = {
      event_whenflagclicked: [t("⚑ when green flag clicked")],
      event_whenkeypressed: [t("when"), f("KEY_OPTION"), t("key pressed")],
      event_whenbroadcastreceived: [t("when I receive"), f("BROADCAST_OPTION")],
      event_whenbackdropswitchesto: [t("when backdrop switches to"), f("BACKDROP")],
      event_whenthisspriteclicked: [t("when this sprite clicked")],
      control_start_as_clone: [t("when I start as a clone")],
      data_setvariableto: [t("set"), f("VARIABLE"), t("to"), i("VALUE")],
      data_changevariableby: [t("change"), f("VARIABLE"), t("by"), i("VALUE")],
      control_if: [t("if"), i("CONDITION"), t("then")],
      control_if_else: [t("if"), i("CONDITION"), t("then")],
      control_repeat_until: [t("repeat until"), i("CONDITION")],
      control_repeat: [t("repeat"), i("TIMES")],
      control_forever: [t("forever")],
      operator_add: [i("NUM1"), t("+"), i("NUM2")],
      operator_subtract: [i("NUM1"), t("−"), i("NUM2")],
      operator_multiply: [i("NUM1"), t("×"), i("NUM2")],
      operator_divide: [i("NUM1"), t("÷"), i("NUM2")],
      operator_gt: [i("OPERAND1"), t(">"), i("OPERAND2")],
      operator_lt: [i("OPERAND1"), t("<"), i("OPERAND2")],
      operator_equals: [i("OPERAND1"), t("="), i("OPERAND2")],
      operator_and: [i("OPERAND1"), t("and"), i("OPERAND2")],
      operator_or: [i("OPERAND1"), t("or"), i("OPERAND2")],
      operator_not: [t("not"), i("OPERAND")],
      data_variable: [f("VARIABLE")],
      argument_reporter_string_number: [f("VALUE")],
      argument_reporter_boolean: [f("VALUE")]
    };
    return table[block.opcode] || [t(block.opcode.replaceAll("_", " "))];
  }

  function renderBlock(id, blocks, mode, seen) {
    const block = blocks[id];
    if (!block || seen.has(id)) return document.createTextNode("");
    const branchSeen = new Set(seen);
    branchSeen.add(id);
    const node = document.createElement("div");
    const boolean = ["operator_gt", "operator_lt", "operator_equals", "operator_and", "operator_or", "operator_not"].includes(block.opcode);
    const hat = block.opcode.startsWith("event_") || block.opcode === "control_start_as_clone";
    const shape = mode === "reporter"
      ? (boolean ? "boolean" : "reporter")
      : (hat ? "hat" : block.opcode.startsWith("control_") ? "control" : "statement");
    node.className = `block ${shape} ${category(block.opcode)}`;
    for (const part of parts(block, blocks)) {
      if (part.kind === "text") {
        const span = document.createElement("span");
        span.textContent = part.value;
        node.append(span);
      } else if (part.kind === "field") {
        const chip = document.createElement("span");
        chip.className = "field";
        chip.textContent = field(block, part.value) || "?";
        node.append(chip);
      } else {
        const value = input(block, part.value);
        const childId = inputBlockId(value);
        if (childId) node.append(renderBlock(childId, blocks, "reporter", branchSeen));
        else {
          const bubble = document.createElement("span");
          bubble.className = "input-empty";
          bubble.textContent = shadowText(value) || " ";
          node.append(bubble);
        }
      }
    }
    return node;
  }

  function renderStack(parent, firstId, blocks, inheritedSeen) {
    let id = firstId;
    const chain = new Set(inheritedSeen || []);
    while (id && blocks[id] && !chain.has(id)) {
      const block = blocks[id];
      const node = renderBlock(id, blocks, "statement", chain);
      parent.append(node);
      chain.add(id);
      if (block.opcode.startsWith("control_")) {
        const substacks = [["SUBSTACK", "then"], ["SUBSTACK2", "else"]];
        for (const [inputName, labelText] of substacks) {
          const childId = inputBlockId(input(block, inputName));
          if (!childId) continue;
          const branch = document.createElement("div");
          branch.className = "branch";
          if (block.opcode === "control_if_else") {
            const label = document.createElement("div");
            label.className = "branch-label";
            label.textContent = labelText;
            branch.append(label);
          }
          renderStack(branch, childId, blocks, chain);
          parent.append(branch);
        }
      }
      id = block.next;
    }
  }

  function showEmpty(title, description) {
    const wrapper = document.createElement("div");
    wrapper.className = "empty";
    const icon = document.createElement("div"); icon.className = "empty-icon"; icon.textContent = "▧";
    const heading = document.createElement("strong"); heading.textContent = title;
    const detail = document.createElement("span"); detail.textContent = description;
    wrapper.append(icon, heading, detail);
    canvas.replaceChildren(wrapper);
  }

  function showCompileError(message) {
    const error = document.createElement("div");
    error.className = "error";
    error.textContent = message;
    canvas.replaceChildren(error);
    count.textContent = "Compile error";
    setStatus("Compilation failed", "error");
  }

  function render(project, blockCount) {
    const target = project.targets.find(item => !item.isStage && Object.keys(item.blocks || {}).length)
      || project.targets.find(item => !item.isStage);
    if (!target) {
      showEmpty("No sprite blocks", "This project has no sprite scripts to preview.");
      count.textContent = `${blockCount} blocks`;
      return;
    }
    const blocks = target.blocks || {};
    const roots = Object.entries(blocks)
      .filter(([, block]) => block.topLevel)
      .sort((a, b) => (a[1].y || 0) - (b[1].y || 0) || (a[1].x || 0) - (b[1].x || 0));
    if (!roots.length) {
      showEmpty("No scripts to preview", "Compile a source file with executable statements.");
      count.textContent = `${blockCount} blocks`;
      return;
    }
    const wrapper = document.createElement("div");
    const title = document.createElement("div"); title.className = "target-title"; title.textContent = target.name || "Sprite";
    wrapper.append(title);
    for (const [id] of roots) {
      const stack = document.createElement("div"); stack.className = "stack";
      renderStack(stack, id, blocks);
      wrapper.append(stack);
    }
    canvas.replaceChildren(wrapper);
    count.textContent = `${blockCount} blocks`;
    setStatus("Compiled successfully", "");
  }

  window.addEventListener("message", event => {
    const message = event.data;
    if (message.type === "compiling") {
      documentName.textContent = message.documentName || "Compiling Rake source";
      setStatus("Compiling Rake source…", "busy");
      return;
    }
    if (message.type === "compiled") {
      documentName.textContent = message.documentName || "Compiled from Rake";
      render(message.project, message.blockCount || 0);
      return;
    }
    if (message.type === "compileError") {
      documentName.textContent = message.documentName || "Compile error";
      showCompileError(message.error || "Compilation failed");
    }
  });
  vscode.postMessage({ type: "ready" });
})();
