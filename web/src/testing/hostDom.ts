// Minimal host nodes for real ReactDOM reconciliation and effects in Node tests.
// Layout and browser event behavior are outside this fixture.
export class HostNode {
  nodeType = 1;
  namespaceURI = 'http://www.w3.org/1999/xhtml';
  parentNode: HostNode | null = null;
  childNodes: HostNode[] = [];
  attributes = new Map<string, string>();
  style = { setProperty() {}, removeProperty() {} };
  value = '';
  checked = false;
  textContent = '';
  constructor(public tagName: string, public ownerDocument: HostDocument) {}
  get nodeName() { return this.tagName; }
  get nodeValue() { return this.textContent; }
  set nodeValue(value: string) { this.textContent = value; }
  appendChild(node: HostNode) { return this.insertBefore(node, null); }
  insertBefore(node: HostNode, before: HostNode | null) {
    node.parentNode?.removeChild(node);
    const index = before ? this.childNodes.indexOf(before) : this.childNodes.length;
    this.childNodes.splice(index, 0, node);
    node.parentNode = this;
    return node;
  }
  removeChild(node: HostNode) { this.childNodes.splice(this.childNodes.indexOf(node), 1); node.parentNode = null; return node; }
  setAttribute(name: string, value: string) { this.attributes.set(name, String(value)); }
  getAttribute(name: string) { return this.attributes.get(name) ?? null; }
  removeAttribute(name: string) { this.attributes.delete(name); }
  addEventListener() {}
  removeEventListener() {}
}
export class HostDocument {
  nodeType = 9;
  documentElement = new HostNode('HTML', this);
  body = new HostNode('BODY', this);
  activeElement = this.body;
  defaultView = { HTMLIFrameElement: class {}, addEventListener() {}, removeEventListener() {} };
  createElement(tag: string) { return new HostNode(tag.toUpperCase(), this); }
  createElementNS(_namespace: string, tag: string) { return this.createElement(tag); }
  createTextNode(text: string) { const node = new HostNode('#text', this); node.nodeType = 3; node.textContent = text; return node; }
  addEventListener() {}
  removeEventListener() {}
}
