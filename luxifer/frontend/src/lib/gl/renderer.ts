// Gemeinsame WebGL-Render-Schicht (ADR 0008): EINE Zeichenschicht für alle
// Ansichten (Design, Preview, Laser). Zeichnet Liniensegmente mit Per-Segment-
// Farbe in EINEM Draw-Call — genau das, was CPU-Canvas nicht schafft (Messung
// ADR 0008: WebGL < 1 ms bei 1 Mio Segmenten inkl. Farbverlauf).
//
// Bewusst rohes WebGL, dünn gekapselt (keine Lib, ADR 0008 §1). 1-px-Linien
// (gl.LINES) — scharf auf jeder Zoomstufe, maximal schnell; dickere Linien
// bräuchten Triangle-Expansion und sind vorerst nicht nötig.

import { type Camera, mmToClipMatrix } from "./camera";

/** Ein Batch aus Liniensegmenten in mm mit Per-Vertex-RGBA-Farbe. */
export interface LineBatch {
  /** Flaches Array [x0,y0, x1,y1, …] in mm (2 Punkte je Segment). */
  positions: Float32Array;
  /** Flaches Array [r,g,b,a, …] je VERTEX (also 2 Farben je Segment), 0..1. */
  colors: Float32Array;
}

const VS = `
attribute vec2 a_pos;
attribute vec4 a_col;
uniform mat3 u_mvp;
varying vec4 v_col;
void main() {
  vec3 p = u_mvp * vec3(a_pos, 1.0);
  gl_Position = vec4(p.xy, 0.0, 1.0);
  gl_PointSize = 9.0;
  v_col = a_col;
}`;

const FS = `
precision mediump float;
varying vec4 v_col;
void main() { gl_FragColor = v_col; }`;

// Textur-Programm (ADR 0008 §2): ein Bild-Quad. a_uv sampelt die 1-Kanal-Textur;
// gebrannte Texel (Wert 1) werden hell, nicht-gebrannte transparent.
const TVS = `
attribute vec2 a_pos;
attribute vec2 a_uv;
uniform mat3 u_mvp;
varying vec2 v_uv;
void main() {
  vec3 p = u_mvp * vec3(a_pos, 1.0);
  gl_Position = vec4(p.xy, 0.0, 1.0);
  v_uv = a_uv;
}`;

const TFS = `
precision mediump float;
uniform sampler2D u_tex;
uniform vec3 u_burn;
varying vec2 v_uv;
void main() {
  float on = texture2D(u_tex, v_uv).r; // 1 = gebrannt
  if (on < 0.5) discard;               // nicht gebrannt = transparent
  gl_FragColor = vec4(u_burn, 1.0);
}`;

/**
 * Kapselt einen WebGL-Kontext + das Linien-Programm. Eine Instanz pro Canvas;
 * die Zeichen-Aufrufe (`begin`/`lines`/`points`) laufen pro Frame.
 */
export class GlRenderer {
  private gl: WebGLRenderingContext;
  private prog: WebGLProgram;
  private locPos: number;
  private locCol: number;
  private locMvp: WebGLUniformLocation;
  // Textur-Programm
  private tprog: WebGLProgram;
  private tPos: number;
  private tUv: number;
  private tMvp: WebGLUniformLocation;
  private tBurn: WebGLUniformLocation;
  private mvp = new Float32Array(9);

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl", { antialias: true, alpha: false });
    if (!gl) throw new Error("WebGL nicht verfügbar");
    this.gl = gl;
    this.prog = linkProgram(gl, VS, FS);
    this.locPos = gl.getAttribLocation(this.prog, "a_pos");
    this.locCol = gl.getAttribLocation(this.prog, "a_col");
    this.locMvp = gl.getUniformLocation(this.prog, "u_mvp")!;
    this.tprog = linkProgram(gl, TVS, TFS);
    this.tPos = gl.getAttribLocation(this.tprog, "a_pos");
    this.tUv = gl.getAttribLocation(this.tprog, "a_uv");
    this.tMvp = gl.getUniformLocation(this.tprog, "u_mvp")!;
    this.tBurn = gl.getUniformLocation(this.tprog, "u_burn")!;
  }

  /** Frame beginnen: Viewport setzen, Hintergrund löschen, Kamera anwenden. */
  begin(cam: Camera, w: number, h: number, bg: [number, number, number]) {
    const gl = this.gl;
    gl.viewport(0, 0, w, h);
    gl.clearColor(bg[0], bg[1], bg[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    this.mvp.set(mmToClipMatrix(cam, w, h));
    gl.useProgram(this.prog);
    gl.uniformMatrix3fv(this.locMvp, false, this.mvp);
    // Alpha-Blending für halbtransparente Linien (Grid/Travel) + Texturen.
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  /**
   * Lädt einen Batch EINMAL in eigene GPU-Buffer hoch und gibt ein Handle
   * zurück. Bei Pan/Zoom wird nur `drawBatch(handle)` gerufen (kein Neu-Upload)
   * — nur die Kamera-Matrix (Uniform) ändert sich. Das ist der Kern der
   * GPU-Performance: Vertex-Daten werden NICHT pro Frame neu kopiert.
   */
  upload(positions: Float32Array, colors: Float32Array): GlBatch {
    const gl = this.gl;
    const pos = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, pos);
    gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW);
    const col = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, col);
    gl.bufferData(gl.ARRAY_BUFFER, colors, gl.STATIC_DRAW);
    return { pos, col, count: positions.length / 2 };
  }

  /** Einen hochgeladenen Batch zeichnen (kein Upload). `mode`: LINES | POINTS. */
  drawBatch(b: GlBatch, mode: "lines" | "points") {
    const gl = this.gl;
    if (b.count === 0) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, b.pos);
    gl.enableVertexAttribArray(this.locPos);
    gl.vertexAttribPointer(this.locPos, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, b.col);
    gl.enableVertexAttribArray(this.locCol);
    gl.vertexAttribPointer(this.locCol, 4, gl.FLOAT, false, 0, 0);
    gl.drawArrays(mode === "lines" ? gl.LINES : gl.POINTS, 0, b.count);
  }

  /** GPU-Buffer eines Batches freigeben (beim Neu-Aufbau der Daten). */
  free(b: GlBatch) {
    this.gl.deleteBuffer(b.pos);
    this.gl.deleteBuffer(b.col);
  }

  /**
   * Lädt eine 1-Kanal-Textur (1 Byte/Texel, 255 = gebrannt) EINMAL hoch samt
   * ihrer mm-Box + Quad/UV-Buffer. `NEAREST`-Sampling → beim Reinzoomen scharfe
   * Pixel (einzelne Rasterzeilen sichtbar, ADR 0008 §2). Wie beim Batch: bei
   * Pan/Zoom wird nur `drawTexture` gerufen, nichts neu hochgeladen.
   */
  uploadTexture(pixels: Uint8Array, w: number, h: number, rect: [number, number, number, number]): GlTexture {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.LUMINANCE, w, h, 0, gl.LUMINANCE, gl.UNSIGNED_BYTE, pixels);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    // Quad aus der mm-Box (zwei Dreiecke) + passende UVs. y↓ im Bild → UV.y umkehren.
    const [x, y, ww, hh] = rect;
    const quad = new Float32Array([
      // pos.x   pos.y     uv.x uv.y
      x,      y,        0, 0,
      x + ww, y,        1, 0,
      x,      y + hh,   0, 1,
      x + ww, y,        1, 0,
      x + ww, y + hh,   1, 1,
      x,      y + hh,   0, 1,
    ]);
    const buf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, quad, gl.STATIC_DRAW);
    return { tex, buf };
  }

  /** Eine hochgeladene Bild-Textur zeichnen (gebrannte Texel in `burn`-Farbe). */
  drawTexture(t: GlTexture, burn: [number, number, number]) {
    const gl = this.gl;
    gl.useProgram(this.tprog);
    gl.uniformMatrix3fv(this.tMvp, false, this.mvp);
    gl.uniform3f(this.tBurn, burn[0], burn[1], burn[2]);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, t.tex);
    gl.bindBuffer(gl.ARRAY_BUFFER, t.buf);
    gl.enableVertexAttribArray(this.tPos);
    gl.vertexAttribPointer(this.tPos, 2, gl.FLOAT, false, 16, 0);
    gl.enableVertexAttribArray(this.tUv);
    gl.vertexAttribPointer(this.tUv, 2, gl.FLOAT, false, 16, 8);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
    // Zurück aufs Linien-Programm für nachfolgende lines()/points().
    gl.useProgram(this.prog);
  }

  /** Textur-Ressourcen freigeben. */
  freeTexture(t: GlTexture) {
    this.gl.deleteTexture(t.tex);
    this.gl.deleteBuffer(t.buf);
  }

  /** Ob der Kontext verloren ist (dann muss neu aufgebaut werden). */
  isLost(): boolean {
    return this.gl.isContextLost();
  }
}

/** Handle auf einen hochgeladenen Batch (eigene GPU-Buffer). */
export interface GlBatch {
  pos: WebGLBuffer;
  col: WebGLBuffer;
  count: number;
}

/** Handle auf eine hochgeladene Bild-Textur (Textur + Quad-Buffer). */
export interface GlTexture {
  tex: WebGLTexture;
  buf: WebGLBuffer;
}

function linkProgram(gl: WebGLRenderingContext, vs: string, fs: string): WebGLProgram {
  const compile = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      throw new Error("Shader-Fehler: " + gl.getShaderInfoLog(s));
    }
    return s;
  };
  const prog = gl.createProgram()!;
  gl.attachShader(prog, compile(gl.VERTEX_SHADER, vs));
  gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    throw new Error("Programm-Link-Fehler: " + gl.getProgramInfoLog(prog));
  }
  return prog;
}
