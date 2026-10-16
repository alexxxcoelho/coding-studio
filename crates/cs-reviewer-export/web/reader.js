"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const decode = (s) => Uint8Array.from(atob(s), c => c.charCodeAt(0));
  let fields = [], active = "", sessionKey = null, revision = 0;
  const localFiles = new Map();
  const valueText = (value) => typeof value === "string" ? value : JSON.stringify(value, null, 2);
  const keyFor = (field) => JSON.stringify(field.path.slice(0, Math.min(2, field.path.length)));
  const labelFor = (field) => field.path.slice(0, Math.min(2, field.path.length)).join(" / ");
  async function download(file, button) {
    const current = revision, key = sessionKey;
    button.disabled = true;
    button.textContent = "Déchiffrement…";
    try {
      // Browsers assemble downloads in memory. Large files remain intact in the
      // package but require an explicit smaller-file export for this reader.
      if (file.size > 1024 * 1024 * 1024) throw new Error("Ce fichier dépasse 1 Gio, la limite de téléchargement de ce lecteur.");
      const parts = [];
      let length = 0;
      for (const chunk of file.chunks) {
        if (revision !== current) throw new Error("Dossier verrouillé.");
        if (!/^[a-f0-9]{64}-\d+\.bin$/.test(chunk.name)) throw new Error("Référence de fichier invalide.");
        let bytes;
        if (localFiles.has(chunk.name)) bytes = await localFiles.get(chunk.name).arrayBuffer();
        else if (location.protocol === "https:" || location.protocol === "http:") {
          const response = await fetch(new URL("data/" + chunk.name, location.href), {redirect:"error",credentials:"omit"});
          if (!response.ok) throw new Error("Pièce jointe absente de l’hébergement.");
          bytes = await response.arrayBuffer();
        } else throw new Error("Choisissez d’abord le dossier « data » ci-dessus.");
        if (revision !== current) throw new Error("Dossier verrouillé.");
        const plain = await crypto.subtle.decrypt({name:"AES-GCM",iv:decode(chunk.iv),additionalData:new TextEncoder().encode(chunk.aad),tagLength:128},key,bytes);
        parts.push(new Uint8Array(plain)); length += plain.byteLength;
      }
      if (length !== file.size) throw new Error("Pièce jointe incomplète.");
      const blob = new Blob(parts, {type:"application/octet-stream"});
      const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer())), b => b.toString(16).padStart(2,"0")).join("");
      if (hash !== file.sha256) throw new Error("L’empreinte de la pièce jointe ne correspond pas.");
      if (revision !== current) throw new Error("Dossier verrouillé.");
      const url = URL.createObjectURL(blob), anchor = document.createElement("a");
      anchor.href = url; anchor.download = file.download_name + ".bin"; anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 10000);
      parts.forEach(part => part.fill(0)); button.textContent = "Télécharger à nouveau";
    } catch (error) { button.textContent = error.name === "OperationError" ? "Pièce jointe endommagée." : error.message; }
    finally { button.disabled = false; }
  }
  function render() {
    const query = $("search").value.trim().toLocaleLowerCase();
    const matches = fields.filter(f => !query || (f.path.join(" / ") + " " + valueText(f.value)).toLocaleLowerCase().includes(query));
    const groups = new Map();
    for (const f of matches) {
      const k = keyFor(f);
      if (!groups.has(k)) groups.set(k, {label:labelFor(f), fields:[]});
      groups.get(k).fields.push(f);
    }
    if (!groups.has(active)) active = groups.keys().next().value || "";
    $("navigation").replaceChildren();
    for (const [key, group] of groups) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = group.label + " (" + group.fields.length + ")";
      if (key === active) button.setAttribute("aria-current", "page");
      button.addEventListener("click", () => { active = key; render(); });
      $("navigation").append(button);
    }
    $("count").textContent = matches.length + " champ" + (matches.length === 1 ? "" : "s") + " sur " + fields.length;
    $("fields").replaceChildren();
    $("empty").hidden = groups.size !== 0;
    $("heading").textContent = groups.get(active)?.label || "";
    for (const f of groups.get(active)?.fields || []) {
      const article = document.createElement("article"); article.className = "field";
      const path = document.createElement("div"); path.className = "path"; path.textContent = f.path.join(" / ");
      const value = document.createElement("div"); value.className = "value";
      if (f.value?.reviewer_file_v1 === true) {
        const button = document.createElement("button"); button.type = "button";
        button.textContent = "Télécharger le fichier (" + (f.value.size / 1024 / 1024).toFixed(1) + " Mio)";
        button.addEventListener("click", () => download(f.value, button)); value.append(button);
      } else value.textContent = valueText(f.value);
      article.append(path, value); $("fields").append(article);
    }
  }
  $("form").addEventListener("submit", async event => {
    event.preventDefault();
    const password = $("password").value;
    $("password").value = "";
    $("unlock").disabled = true;
    $("status").textContent = "Ouverture du dossier…";
    try {
      if (!globalThis.crypto?.subtle) throw new Error("secure-context");
      const e = JSON.parse(new TextDecoder().decode(decode($("encrypted").textContent)));
      if (e.format !== "coding-studio-reviewer-aes256gcm-v1" || e.iterations !== 600000) throw new Error("format");
      const salt = decode(e.salt), iv = decode(e.iv);
      if (salt.length !== 16 || iv.length !== 12) throw new Error("format");
      const material = await crypto.subtle.importKey("raw", new TextEncoder().encode(password), "PBKDF2", false, ["deriveKey"]);
      const key = await crypto.subtle.deriveKey({name:"PBKDF2", salt, iterations:e.iterations, hash:"SHA-256"}, material, {name:"AES-GCM", length:256}, false, ["decrypt"]);
      const plain = await crypto.subtle.decrypt({name:"AES-GCM", iv, additionalData:new TextEncoder().encode("CodingStudio reviewer export v1"), tagLength:128}, key, decode(e.ciphertext));
      const payload = JSON.parse(new TextDecoder("utf-8", {fatal:true}).decode(plain));
      new Uint8Array(plain).fill(0);
      if (payload.format !== "coding-studio-reviewer-data-v1" || !Array.isArray(payload.fields) || !payload.fields.every(f => Array.isArray(f.path) && f.path.length && f.path.every(p => typeof p === "string") && Object.hasOwn(f, "value"))) throw new Error("format");
      fields = payload.fields; active = ""; sessionKey = key; revision++;
      $("export-note").hidden = !payload.export_info;
      $("export-note").textContent = payload.export_info?.pseudonymised_text
        ? "Copie avec pseudonymisation des textes (" + payload.export_info.replacements + " remplacements). Les fichiers originaux ne sont pas pseudonymisés. Les empreintes enregistrées concernent les originaux, avant remplacement des noms."
        : "Copie sans pseudonymisation des textes. Les empreintes enregistrées concernent les originaux.";
      $("local-files").hidden = !fields.some(f => f.value?.reviewer_file_v1 === true);
      $("gate").hidden = true; $("reader").hidden = false; $("status").textContent = "";
      render(); $("search").focus();
    } catch (error) {
      $("status").textContent = error.message === "secure-context"
        ? "Le navigateur ne permet pas le déchiffrement ici. Ouvrez ce dossier depuis une adresse HTTPS ou un serveur local."
        : "Impossible d’ouvrir le dossier : mot de passe incorrect ou fichier endommagé.";
    } finally { $("unlock").disabled = false; }
  });
  $("search").addEventListener("input", render);
  $("source-folder").addEventListener("change", event => {
    localFiles.clear();
    for (const file of event.target.files) if (/^[a-f0-9]{64}-\d+\.bin$/.test(file.name)) localFiles.set(file.name, file);
  });
  $("lock").addEventListener("click", () => {
    fields = []; active = ""; sessionKey = null; revision++; localFiles.clear(); $("source-folder").value = "";
    $("fields").replaceChildren(); $("navigation").replaceChildren();
    $("search").value = ""; $("heading").textContent = ""; $("count").textContent = "";
    $("export-note").textContent = ""; $("reader").hidden = true; $("gate").hidden = false; $("password").focus();
  });
})();
