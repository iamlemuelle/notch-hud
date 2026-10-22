/* @bible-strong/avatar-core 0.1.0 (AGPL-3.0-only), https://github.com/smontlouis/bible-strong-avatar-lab. Bundled by scripts/vendor-avatar-core.sh with esbuild 0.28.2; ajv replaced by a stub. */
var AvatarCore = (() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

  // entry.js
  var entry_exports = {};
  __export(entry_exports, {
    advanceAvatarPlayback: () => Me,
    playAvatarAnimation: () => je,
    renderAvatarDefinition: () => ke,
    renderAvatarFrame: () => Fe,
    sampleAvatarFrame: () => $2
  });

  // pkg/package/dist/ambientMotion.js
  var e = [
    "none",
    "microSaccades",
    "shake"
  ];
  var t = [
    "none",
    "slowDrift",
    "shake"
  ];
  var n = new Set(e);
  var r = new Set(t);
  var o = (e2) => e2 * e2 * (3 - 2 * e2);
  var s = (e2) => {
    let t2 = Math.sin(e2 * 127.1 + 311.7) * 43758.5453;
    return (t2 - Math.floor(t2)) * 2 - 1;
  };
  var c = (e2) => e2.headX * 0.71 + e2.headY * 1.13 + e2.headZ * 1.37;
  var l = 17.29;
  var u = (e2, t2, n3, r3) => {
    let i4 = e2 / r3, a4 = Math.floor(i4), c4 = o(i4 - a4), l4 = s(a4 * 3 + t2 + n3);
    return l4 + (s((a4 + 1) * 3 + t2 + n3) - l4) * c4;
  };
  var d = (e2, t2, n3) => {
    let r3 = 1100;
    if (e2 <= 0) return 0;
    let i4 = Math.floor(e2 / r3), a4 = (e2 - i4 * r3) / 140, c4 = o(Math.min(a4, 1)), l4 = i4 === 0 ? 0 : s((i4 - 1) * 2 + t2 + n3);
    return l4 + (s(i4 * 2 + t2 + n3) - l4) * c4;
  };
  var m = (e2, t2, n3 = 1) => {
    if (e2.eyeMotion === "microSaccades") return {
      x: d(t2, 0, l) * 1.5 * n3,
      y: d(t2, 1, l) * 0.9 * n3
    };
    if (e2.eyeMotion === "shake") {
      let e3 = t2 / 1e3;
      return {
        x: (Math.sin(e3 * 47) + Math.sin(e3 * 71) * 0.45) * 1.2 * n3,
        y: (Math.sin(e3 * 59) + Math.sin(e3 * 83) * 0.4) * 0.8 * n3
      };
    }
    return {
      x: 0,
      y: 0
    };
  };
  var h = (e2, t2, n3 = 1) => {
    let r3 = { ...e2 }, i4 = c(e2);
    if (e2.bodyMotion === "slowDrift") r3.headX += u(t2, 0, i4, 2600) * 0.8 * n3, r3.headY += u(t2, 1, i4, 3300) * 1.15 * n3, r3.headZ += u(t2, 2, i4, 4100) * 0.45 * n3;
    else if (e2.bodyMotion === "shake") {
      let e3 = t2 / 1e3;
      r3.headX += (Math.sin(e3 * 31) + Math.sin(e3 * 53) * 0.45) * 1.15 * n3, r3.headY += (Math.sin(e3 * 37) + Math.sin(e3 * 61) * 0.4) * 1.35 * n3, r3.headZ += Math.sin(e3 * 43) * 0.7 * n3;
    }
    return r3;
  };
  var g = (e2, t2, n3 = 1) => {
    let r3 = h(e2, t2, n3), i4 = m(e2, t2, n3);
    return r3.positionXLeft += i4.x, r3.positionXRight += i4.x, r3.positionYLeft += i4.y, r3.positionYRight += i4.y, r3;
  };

  // pkg/package/dist/surfaces.js
  var n2 = (e2, t2) => Math.sign(e2) * Math.abs(e2) ** t2;
  var r2 = (e2, t2, r3, i4, a4, o4, s4) => {
    let c4 = n2(Math.cos(t2), o4);
    return [
      r3 / 2 * c4 * n2(Math.sin(e2), s4),
      i4 / 2 * n2(Math.sin(t2), o4),
      a4 / 2 * c4 * n2(Math.cos(e2), s4)
    ];
  };
  var i = (e2, t2, n3) => {
    let r3 = e2.width / 2, i4 = e2.depth / 2, a4 = Math.min(r3, e2.height / 2), o4 = Math.max(0, (e2.height - a4 * 2) / 2), s4 = o4 * 2 + Math.PI * a4, c4 = (n3 + Math.PI / 2) / Math.PI * s4, l4 = r3, u4 = 0;
    if (c4 < Math.PI * a4 / 2) {
      let e3 = -Math.PI / 2 + c4 / a4;
      l4 = r3 * Math.cos(e3), u4 = -o4 + a4 * Math.sin(e3);
    } else if (c4 <= Math.PI * a4 / 2 + o4 * 2) u4 = -o4 + c4 - Math.PI * a4 / 2;
    else {
      let e3 = (c4 - Math.PI * a4 / 2 - o4 * 2) / a4;
      l4 = r3 * Math.cos(e3), u4 = o4 + a4 * Math.sin(e3);
    }
    let d4 = r3 ? i4 / r3 : 1;
    return [
      l4 * Math.sin(t2),
      u4,
      l4 * d4 * Math.cos(t2)
    ];
  };
  var a = (e2) => Math.max(0, Math.min(2, e2 ?? 0));
  var o2 = (e2) => 1 + a(e2.roundness) / 2;
  var s2 = 0.04;
  var c2 = (e2) => e2.roundness <= 0 ? Infinity : 2 / (s2 + a(e2.roundness) / 2 * 0.96);
  var l2 = (e2, t2, n3, r3) => {
    let i4 = Math.cos(n3) * Math.sin(t2), a4 = Math.sin(n3), o4 = Math.cos(n3) * Math.cos(t2), s4 = Number.isFinite(r3) ? (Math.abs(i4) ** r3 + Math.abs(a4) ** r3 + Math.abs(o4) ** r3) ** (1 / r3) || 1 : Math.max(Math.abs(i4), Math.abs(a4), Math.abs(o4)) || 1;
    return [
      e2.width / 2 * (i4 / s4),
      e2.height / 2 * (a4 / s4),
      e2.depth / 2 * (o4 / s4)
    ];
  };
  var u2 = (e2, t2, n3) => l2(e2, t2, n3, o2(e2));
  var d2 = (e2, t2, n3) => l2(e2, t2, n3, c2(e2));
  var f = 0.24;
  var p = 0.2;
  var m2 = 0.22;
  var h2 = (e2) => a(e2.morphRoundness) / 2;
  var g2 = (e2, t2, n3) => {
    let r3 = h2(e2), i4 = Math.max(0, Math.min(1, t2)), a4 = Math.sin(i4 * Math.PI), o4 = (1 - Math.cos(i4 * Math.PI)) / 2;
    return {
      radiusScale: n3.radiusScale + (a4 - n3.radiusScale) * r3,
      verticalProgress: n3.verticalProgress + (o4 - n3.verticalProgress) * r3
    };
  };
  var _ = (e2, t2, n3, r3, i4) => {
    let a4 = 1 - i4;
    return a4 ** 3 * e2 + 3 * a4 * a4 * i4 * t2 + 3 * a4 * i4 * i4 * n3 + i4 ** 3 * r3;
  };
  var v = (e2) => ({
    tipFraction: (e2.tipRoundness ?? 0) * f,
    baseFraction: (e2.baseRoundness ?? 0) * p
  });
  var y = (e2, t2) => {
    let n3 = Math.max(0, Math.min(1, t2)), r3 = e2.roundness * m2;
    if (r3 <= 0) return {
      radiusScale: 1,
      verticalProgress: (Math.sin((n3 - 0.5) * Math.PI) + 1) / 2
    };
    if (n3 < r3) {
      let e3 = -Math.PI / 2 + n3 / r3 * (Math.PI / 2);
      return {
        radiusScale: 1 - r3 + r3 * Math.cos(e3),
        verticalProgress: (r3 + r3 * Math.sin(e3)) / 2
      };
    }
    if (n3 > 1 - r3) {
      let e3 = (n3 - (1 - r3)) / r3 * (Math.PI / 2);
      return {
        radiusScale: 1 - r3 + r3 * Math.cos(e3),
        verticalProgress: 1 - r3 / 2 + r3 * Math.sin(e3) / 2
      };
    }
    let i4 = (n3 - r3) / (1 - r3 * 2);
    return {
      radiusScale: 1,
      verticalProgress: r3 / 2 + i4 * (1 - r3)
    };
  };
  var b = (e2, t2) => g2(e2, t2, y(e2, t2));
  var x = (e2, t2, n3) => {
    let r3 = Math.max(0, Math.min(1, t2)), i4 = 0, a4 = 1;
    for (let t3 = 0; t3 < 14; t3 += 1) {
      let t4 = (i4 + a4) / 2;
      n3(e2, t4).verticalProgress < r3 ? i4 = t4 : a4 = t4;
    }
    return n3(e2, (i4 + a4) / 2).radiusScale;
  };
  var S = (e2, t2) => {
    let n3 = Math.max(0, Math.min(1, t2)), { tipFraction: r3, baseFraction: i4 } = v(e2);
    if (i4 > 0 && n3 < i4) {
      let e3 = n3 / i4;
      return {
        radiusScale: _(1 - i4, 1, 1 - i4 / 2, 1 - i4, e3),
        verticalProgress: _(0, 0, i4 / 2, i4, e3)
      };
    }
    if (r3 > 0 && n3 > 1 - r3) {
      let e3 = (n3 - (1 - r3)) / r3;
      return {
        radiusScale: _(r3, r3 / 2, r3 / 4, 0, e3),
        verticalProgress: _(1 - r3, 1 - r3 / 2, 1, 1, e3)
      };
    }
    return {
      radiusScale: 1 - n3,
      verticalProgress: n3
    };
  };
  var C = (e2, t2) => g2(e2, t2, S(e2, t2));
  var w = (e2) => {
    let t2 = e2.height * 0.36, n3 = e2.height - t2;
    return {
      coneApexY: -e2.height / 2,
      coneBaseY: -e2.height / 2 + t2,
      bodyHeight: n3,
      bodyCenterY: e2.height / 2 - n3 / 2,
      bodyWidth: e2.width * 0.54,
      bodyDepth: e2.depth * 0.62
    };
  };
  var T = (e2, t2, n3) => {
    let { width: a4, height: o4, depth: s4 } = e2;
    switch (e2.type) {
      case "sphere":
      case "mickey":
        return r2(t2, n3, a4, o4, s4, 1, 1);
      case "cube":
        return d2(e2, t2, n3);
      case "cylinder": {
        let r3 = b(e2, (n3 + Math.PI / 2) / Math.PI);
        return [
          a4 / 2 * r3.radiusScale * Math.sin(t2),
          -o4 / 2 + o4 * r3.verticalProgress,
          s4 / 2 * r3.radiusScale * Math.cos(t2)
        ];
      }
      case "cursor": {
        let r3 = w(e2), i4 = (n3 + Math.PI / 2) / Math.PI, a5 = y({
          ...e2,
          width: r3.bodyWidth,
          height: r3.bodyHeight,
          depth: r3.bodyDepth
        }, i4);
        return [
          r3.bodyWidth / 2 * a5.radiusScale * Math.sin(t2),
          r3.bodyCenterY - r3.bodyHeight / 2 + r3.bodyHeight * a5.verticalProgress,
          r3.bodyDepth / 2 * a5.radiusScale * Math.cos(t2)
        ];
      }
      case "diamond":
        return u2(e2, t2, n3);
      case "capsule":
        return i(e2, t2, n3);
      case "cone": {
        let r3 = C(e2, (n3 + Math.PI / 2) / Math.PI);
        return [
          a4 / 2 * r3.radiusScale * Math.sin(t2),
          o4 / 2 - o4 * r3.verticalProgress,
          s4 / 2 * r3.radiusScale * Math.cos(t2)
        ];
      }
    }
  };
  var E = (e2, t2) => [
    e2[0] - t2[0],
    e2[1] - t2[1],
    e2[2] - t2[2]
  ];
  var D = ([e2, t2, n3]) => {
    let r3 = Math.hypot(e2, t2, n3) || 1;
    return [
      e2 / r3,
      t2 / r3,
      n3 / r3
    ];
  };
  var O = (e2, t2, n3) => {
    let r3 = e2.type === "cone" ? -1 : 1;
    return D([
      r3 * (t2[1] * n3[2] - t2[2] * n3[1]),
      r3 * (t2[2] * n3[0] - t2[0] * n3[2]),
      r3 * (t2[0] * n3[1] - t2[1] * n3[0])
    ]);
  };
  var k = (e2, t2, n3) => {
    let r3 = 5e-4;
    if (e2.type === "cone" && n3 >= Math.PI / 2 - r3) return [
      0,
      -1,
      0
    ];
    let i4 = T(e2, t2 - r3, n3), a4 = T(e2, t2 + r3, n3), o4 = T(e2, t2, Math.max(-Math.PI / 2, n3 - r3)), s4 = T(e2, t2, Math.min(Math.PI / 2, n3 + r3));
    return O(e2, E(a4, i4), E(s4, o4));
  };
  var A = (e2, t2) => Math.sign(e2) * Math.abs(e2) ** t2;
  var j = (e2, t2, n3) => {
    let r3 = e2.width / 2 || 1, i4 = e2.height / 2 || 1, a4 = e2.depth / 2 || 1;
    return D([
      A(t2[0] / r3, n3 - 1) / r3,
      A(t2[1] / i4, n3 - 1) / i4,
      A(t2[2] / a4, n3 - 1) / a4
    ]);
  };
  var M = (e2, t2) => j(e2, t2, o2(e2));
  var N = (e2, t2) => {
    let n3 = c2(e2);
    if (Number.isFinite(n3)) return j(e2, t2, n3);
    let r3 = [
      t2[0] / (e2.width / 2 || 1),
      t2[1] / (e2.height / 2 || 1),
      t2[2] / (e2.depth / 2 || 1)
    ], i4 = r3.reduce((e3, t3, n4) => Math.abs(t3) > Math.abs(r3[e3]) ? n4 : e3, 0);
    return [
      i4 === 0 ? Math.sign(r3[0]) : 0,
      i4 === 1 ? Math.sign(r3[1]) : 0,
      i4 === 2 ? Math.sign(r3[2]) : 0
    ];
  };
  var P = (e2, t2, n3, r3, i4) => {
    let a4 = e2.width / 2 || 1, o4 = e2.height / 2 || 1, s4 = e2.depth / 2 || 1;
    if (!Number.isFinite(r3)) {
      let r4 = [
        Math.max(-a4, Math.min(a4, t2)),
        Math.max(-o4, Math.min(o4, n3)),
        s4
      ];
      return {
        point: r4,
        normal: i4(e2, r4)
      };
    }
    let c4 = Math.max(-1, Math.min(1, n3 / o4)), l4 = Math.max(0, 1 - Math.abs(c4) ** r3) ** (1 / r3), u4 = Math.max(-a4 * l4, Math.min(a4 * l4, t2)), d4 = u4 / a4, f4 = Math.max(0, 1 - Math.abs(d4) ** r3 - Math.abs(c4) ** r3) ** (1 / r3), p4 = [
      u4,
      c4 * o4,
      s4 * f4
    ];
    return {
      point: p4,
      normal: i4(e2, p4)
    };
  };
  var F = (e2, t2, n3, r3, i4, a4 = 0) => {
    let o4 = t2 - a4, s4 = Math.max(0, 1 - (e2 / (n3 || 1)) ** 2 - (o4 / (r3 || 1)) ** 2), c4 = i4 * Math.sqrt(s4);
    return {
      point: [
        e2,
        t2,
        c4
      ],
      normal: D([
        e2 / (n3 * n3 || 1),
        o4 / (r3 * r3 || 1),
        c4 / (i4 * i4 || 1)
      ])
    };
  };
  var I = (e2, t2, n3, r3, i4) => {
    let a4 = e2.width / 2 || 1, o4 = e2.depth / 2 || 1, s4 = Math.max(0, Math.min(1, 0.5 + i4 * (n3 / e2.height))), c4 = x(e2, s4, r3), l4 = a4 * c4, u4 = o4 * c4, d4 = Math.max(-l4, Math.min(l4, t2)), f4 = l4 > 0 ? Math.max(0, 1 - (d4 / l4) ** 2) : 0, p4 = u4 * Math.sqrt(f4), m4 = 1e-4, h4 = Math.max(0, s4 - m4), g4 = Math.min(1, s4 + m4), _3 = x(e2, h4, r3), v3 = (x(e2, g4, r3) - _3) / (g4 - h4 || 1), y3 = Math.max(Math.sqrt(f4), 1e-4), b3 = -(o4 / a4) * d4 / (l4 * y3 || 1), S3 = i4 * o4 * v3 / (e2.height * y3 || 1);
    return {
      point: [
        d4,
        n3,
        p4
      ],
      normal: D([
        -b3,
        -S3,
        1
      ])
    };
  };
  var L = (e2, t2, n3) => {
    let r3 = e2.width / 2 || 1, i4 = e2.height / 2 || 1, a4 = e2.depth / 2 || 1;
    switch (e2.type) {
      case "sphere":
      case "mickey":
        return F(t2, n3, r3, i4, a4);
      case "cube":
        return P(e2, t2, n3, c2(e2), N);
      case "capsule": {
        let e3 = Math.min(r3, i4), o4 = Math.max(0, i4 - e3);
        return F(t2, n3, r3, e3, a4, n3 < -o4 ? -o4 : n3 > o4 ? o4 : n3);
      }
      case "cylinder":
        return I(e2, t2, n3, b, 1);
      case "cursor": {
        let r4 = w(e2), i5 = I({
          ...e2,
          width: r4.bodyWidth,
          height: r4.bodyHeight,
          depth: r4.bodyDepth
        }, t2, n3 - r4.bodyCenterY, y, 1);
        return {
          point: [
            i5.point[0],
            i5.point[1] + r4.bodyCenterY,
            i5.point[2]
          ],
          normal: i5.normal
        };
      }
      case "cone":
        return I(e2, t2, n3, C, -1);
      case "diamond":
        return P(e2, t2, n3, o2(e2), M);
    }
  };
  var z = (e2, t2, n3) => {
    let r3 = T(e2, t2, n3);
    if (e2.type === "sphere" || e2.type === "mickey") {
      let t3 = e2.width / 2 || 1, n4 = e2.height / 2 || 1, i4 = e2.depth / 2 || 1;
      return {
        point: r3,
        normal: D([
          r3[0] / (t3 * t3),
          r3[1] / (n4 * n4),
          r3[2] / (i4 * i4)
        ])
      };
    }
    return e2.type === "cylinder" && e2.roundness <= 0 && (e2.morphRoundness ?? 0) <= 0 ? {
      point: r3,
      normal: D([
        Math.sin(t2) / (e2.width / 2 || 1),
        0,
        Math.cos(t2) / (e2.depth / 2 || 1)
      ])
    } : e2.type === "diamond" ? {
      point: r3,
      normal: M(e2, r3)
    } : e2.type === "cube" ? {
      point: r3,
      normal: N(e2, r3)
    } : {
      point: r3,
      normal: k(e2, t2, n3)
    };
  };

  // pkg/package/dist/geometry.js
  var a2 = 620;
  var o3 = 14;
  var s3 = [
    "headX",
    "headY",
    "headZ",
    "widthLeft",
    "widthRight",
    "heightLeft",
    "heightRight",
    "spacing",
    "positionXLeft",
    "positionXRight",
    "positionYLeft",
    "positionYRight",
    "leftAngle",
    "rightAngle",
    "perspective"
  ];
  var c3 = (e2, t2, n3) => Math.max(t2, Math.min(n3, e2));
  var l3 = (e2) => e2 * Math.PI / 180;
  var u3 = ([e2, t2, n3, r3]) => {
    let i4 = Math.hypot(e2, t2, n3, r3) || 1;
    return [
      e2 / i4,
      t2 / i4,
      n3 / i4,
      r3 / i4
    ];
  };
  var d3 = ([e2, t2, n3, r3], [i4, a4, o4, s4]) => u3([
    e2 * i4 - t2 * a4 - n3 * o4 - r3 * s4,
    e2 * a4 + t2 * i4 + n3 * s4 - r3 * o4,
    e2 * o4 - t2 * s4 + n3 * i4 + r3 * a4,
    e2 * s4 + t2 * o4 - n3 * a4 + r3 * i4
  ]);
  var f2 = ([e2, t2, n3], r3) => {
    let i4 = r3 / 2, a4 = Math.sin(i4);
    return u3([
      Math.cos(i4),
      e2 * a4,
      t2 * a4,
      n3 * a4
    ]);
  };
  var p2 = (e2, t2, n3) => {
    let r3 = f2([
      1,
      0,
      0
    ], e2), i4 = f2([
      0,
      1,
      0
    ], t2);
    return d3(d3(f2([
      0,
      0,
      1
    ], n3), r3), i4);
  };
  var g3 = (e2, t2) => {
    let n3 = e2;
    for (; n3 - t2 > 180; ) n3 -= 360;
    for (; n3 - t2 < -180; ) n3 += 360;
    return c3(n3, -365, 365);
  };
  var y2 = ([e2, t2, n3, r3], [i4, a4, o4]) => {
    let s4 = 2 * (n3 * o4 - r3 * a4), c4 = 2 * (r3 * i4 - t2 * o4), l4 = 2 * (t2 * a4 - n3 * i4);
    return [
      i4 + e2 * s4 + (n3 * l4 - r3 * c4),
      a4 + e2 * c4 + (r3 * s4 - t2 * l4),
      o4 + e2 * l4 + (t2 * c4 - n3 * s4)
    ];
  };
  var b2 = (e2, t2) => {
    let n3 = e2 / 2, r3 = t2 / 2, i4 = Math.min(r3, n3), a4 = [], s4 = (e3, t3) => {
      let n4 = Math.max(2, Math.ceil(Math.hypot(t3[0] - e3[0], t3[1] - e3[1]) / 1.5));
      for (let r4 = 0; r4 < n4; r4 += 1) {
        let i5 = r4 / n4;
        a4.push([e3[0] + (t3[0] - e3[0]) * i5, e3[1] + (t3[1] - e3[1]) * i5]);
      }
    }, c4 = (e3, t3, n4) => {
      for (let r4 = 0; r4 < o3; r4 += 1) {
        let s5 = n4 + r4 / o3 * (Math.PI / 2);
        a4.push([e3 + Math.cos(s5) * i4, t3 + Math.sin(s5) * i4]);
      }
    };
    return s4([-n3 + i4, -r3], [n3 - i4, -r3]), c4(n3 - i4, -r3 + i4, -Math.PI / 2), s4([n3, -r3 + i4], [n3, r3 - i4]), c4(n3 - i4, r3 - i4, 0), s4([n3 - i4, r3], [-n3 + i4, r3]), c4(-n3 + i4, r3 - i4, Math.PI / 2), s4([-n3, r3 - i4], [-n3, -r3 + i4]), c4(-n3 + i4, -r3 + i4, Math.PI), a4;
  };
  var x2 = (e2, t2) => {
    let n3 = a2 - e2[2] * t2, r3 = Math.abs(n3) < 1e-4 ? a2 / 1e-4 : a2 / n3;
    return [
      e2[0] * r3,
      e2[1] * r3,
      e2[2]
    ];
  };
  var D2 = (e2, t2 = true) => e2.length ? `M${e2[0][0].toFixed(2)} ${e2[0][1].toFixed(2)}${e2.slice(1).map((e3) => `L${e3[0].toFixed(2)} ${e3[1].toFixed(2)}`).join("")}${t2 ? "Z" : ""}` : "";
  var O2 = (e2) => ({
    expression: e2,
    orientation: p2(l3(e2.headX), l3(e2.headY), l3(e2.headZ))
  });
  var ae = (e2, t2, n3) => {
    let r3 = { ...e2.expression };
    return s3.forEach((i4) => {
      let a4 = t2.expression[i4];
      (i4 === "headX" || i4 === "headY" || i4 === "headZ" || i4 === "leftAngle" || i4 === "rightAngle") && (a4 = g3(a4, e2.expression[i4])), r3[i4] = e2.expression[i4] + (a4 - e2.expression[i4]) * n3;
    }), {
      expression: r3,
      orientation: O2(r3).orientation
    };
  };
  var oe = 24;
  var se = 25;
  var ce = 73;
  var k2 = 144;
  var le = 33;
  var ue = 73;
  var A2 = /* @__PURE__ */ new Map();
  var j2 = /* @__PURE__ */ new Map();
  var M2 = /* @__PURE__ */ new Map();
  var N2 = (e2) => JSON.stringify([
    e2.type,
    e2.width,
    e2.height,
    e2.depth,
    e2.roundness,
    e2.morphRoundness,
    e2.tipRoundness,
    e2.baseRoundness
  ]);
  var P2 = (e2, t2, n3) => (e2.size >= oe && e2.delete(e2.keys().next().value), e2.set(t2, n3), n3);
  var F2 = (e2, t2, n3) => z(e2, t2, n3);
  var I2 = (e2, t2) => ({
    point: x2(y2(e2.orientation, t2.point), e2.expression.perspective),
    normal: y2(e2.orientation, t2.normal)
  });
  var de = (e2, t2) => {
    let n3 = e2 / 120, r3 = t2 / 120;
    return [120 * Math.cos(r3) * Math.sin(n3), 120 * Math.sin(r3)];
  };
  var L2 = (e2, n3, r3, i4) => {
    let [a4, o4] = de(r3, i4);
    return I2(e2, L(n3, a4, o4));
  };
  var R = (e2, t2, n3, r3, i4 = {
    x: 0,
    y: 0
  }) => {
    let a4 = e2.expression, o4 = n3 < 0 ? "Left" : "Right", s4 = a4[`width${o4}`], c4 = 5 + (a4[`height${o4}`] - 5) * r3, u4 = n3 * a4.spacing / 2 + a4[`positionX${o4}`] + i4.x, d4 = a4[`positionY${o4}`] + i4.y, f4 = l3(n3 < 0 ? a4.leftAngle : a4.rightAngle);
    return b2(s4, c4).map(([n4, r4]) => {
      let i5 = n4 * Math.cos(f4) - r4 * Math.sin(f4), a5 = n4 * Math.sin(f4) + r4 * Math.cos(f4);
      return L2(e2, t2, u4 + i5, d4 + a5);
    });
  };
  var z2 = (e2) => {
    let t2 = [], n3 = [];
    return e2.forEach(({ point: e3, normal: r3 }) => {
      r3[2] > 0 ? n3.push(e3) : n3.length && (t2.push(n3), n3 = []);
    }), n3.length && t2.push(n3), t2.filter((e3) => e3.length > 1).map((e3) => D2(e3, false)).join("");
  };
  var B = (e2, t2) => {
    let n3 = N2(t2), r3 = M2.get(n3);
    if (!r3) {
      let e3 = [
        -60,
        -30,
        0,
        30,
        60
      ].map((e4) => Array.from({ length: 73 }, (n4, r4) => F2(t2, l3(-180 + r4 * 5), l3(e4)))), i4 = Array.from({ length: 12 }, (e4, t3) => -150 + t3 * 30).map((e4) => Array.from({ length: 37 }, (n4, r4) => F2(t2, l3(e4), l3(-90 + r4 * 5))));
      r3 = P2(M2, n3, [...e3, ...i4]);
    }
    return r3.map((t3) => z2(t3.map((t4) => I2(e2, t4))));
  };
  var U = (e2) => {
    let t2 = [...e2].sort((e3, t3) => e3[0] - t3[0] || e3[1] - t3[1]), n3 = (e3, t3, n4) => (t3[0] - e3[0]) * (n4[1] - e3[1]) - (t3[1] - e3[1]) * (n4[0] - e3[0]), r3 = (e3) => {
      let t3 = [];
      return e3.forEach((e4) => {
        for (; t3.length >= 2 && n3(t3.at(-2), t3.at(-1), e4) <= 0; ) t3.pop();
        t3.push(e4);
      }), t3;
    };
    return [...r3(t2).slice(0, -1), ...r3(t2.reverse()).slice(0, -1)];
  };
  var W = (e2) => {
    if (e2.length < 3) return D2(e2);
    let t2 = (t3) => e2[(t3 + e2.length) % e2.length];
    return `M${e2[0][0].toFixed(2)} ${e2[0][1].toFixed(2)}${e2.map((e3, n3) => {
      let r3 = t2(n3 - 1), i4 = t2(n3 + 1), a4 = t2(n3 + 2), o4 = [
        e3[0] + (i4[0] - r3[0]) / 6,
        e3[1] + (i4[1] - r3[1]) / 6,
        e3[2]
      ], s4 = [
        i4[0] - (a4[0] - e3[0]) / 6,
        i4[1] - (a4[1] - e3[1]) / 6,
        i4[2]
      ];
      return `C${o4[0].toFixed(2)} ${o4[1].toFixed(2)} ${s4[0].toFixed(2)} ${s4[1].toFixed(2)} ${i4[0].toFixed(2)} ${i4[1].toFixed(2)}`;
    }).join("")}Z`;
  };
  var G = (e2, t2 = 7) => e2.flatMap((n3, r3) => {
    let i4 = e2[(r3 + 1) % e2.length], a4 = Math.max(1, Math.ceil(Math.hypot(i4[0] - n3[0], i4[1] - n3[1]) / t2));
    return Array.from({ length: a4 }, (e3, t3) => {
      let r4 = t3 / a4;
      return [
        n3[0] + (i4[0] - n3[0]) * r4,
        n3[1] + (i4[1] - n3[1]) * r4,
        n3[2] + (i4[2] - n3[2]) * r4
      ];
    });
  });
  var fe = (e2) => e2.length ? e2.length === 1 ? `${e2[0][0].toFixed(2)} ${e2[0][1].toFixed(2)}` : e2.slice(0, -1).map((t2, n3) => {
    let r3 = e2[Math.max(0, n3 - 1)], i4 = e2[n3 + 1], a4 = e2[Math.min(e2.length - 1, n3 + 2)], o4 = t2[0] + (i4[0] - r3[0]) / 6, s4 = t2[1] + (i4[1] - r3[1]) / 6, c4 = i4[0] - (a4[0] - t2[0]) / 6, l4 = i4[1] - (a4[1] - t2[1]) / 6;
    return `C${o4.toFixed(2)} ${s4.toFixed(2)} ${c4.toFixed(2)} ${l4.toFixed(2)} ${i4[0].toFixed(2)} ${i4[1].toFixed(2)}`;
  }).join("") : "";
  var K = (e2, t2) => x2(y2(e2.orientation, t2), e2.expression.perspective);
  var q = (e2, t2, n3) => Array.from({ length: 145 }, (r3, i4) => {
    let a4 = i4 / k2 * Math.PI * 2;
    return [
      e2 / 2 * Math.sin(a4),
      n3,
      t2 / 2 * Math.cos(a4)
    ];
  });
  var J = (e2, t2) => {
    let r3 = N2(t2), i4 = A2.get(r3);
    return i4 || (i4 = Array.from({ length: le }, (e3, r4) => {
      let i5 = -Math.PI / 2 + r4 / 32 * Math.PI;
      return Array.from({ length: ue }, (e4, r5) => {
        let a4 = -Math.PI + r5 / 72 * Math.PI * 2;
        return T(t2, a4, i5);
      });
    }).flat(), P2(A2, r3, i4)), W(G(U(i4.map((t3) => K(e2, t3)))));
  };
  var pe = (e2, t2) => {
    if (t2.roundness > 0 || (t2.morphRoundness ?? 0) > 0) return J(e2, t2);
    let n3 = t2.height / 2;
    return W(G(U([...q(t2.width, t2.depth, -n3), ...q(t2.width, t2.depth, n3)].map((t3) => K(e2, t3)))));
  };
  var me = (t2, n3) => {
    let r3 = w(n3), i4 = r3.bodyHeight / 2;
    return W(G(U([...q(r3.bodyWidth, r3.bodyDepth, r3.bodyCenterY - i4), ...q(r3.bodyWidth, r3.bodyDepth, r3.bodyCenterY + i4)].map((e2) => K(t2, e2)))));
  };
  var he = (t2, n3) => {
    let r3 = w(n3), i4 = K(t2, [
      0,
      r3.coneApexY,
      0
    ]);
    return W(G(U([...q(n3.width, n3.depth, r3.coneBaseY).map((e2) => K(t2, e2)), i4])));
  };
  var ge = (e2, t2) => {
    if ((t2.morphRoundness ?? 0) > 0 || (t2.tipRoundness ?? 0) > 0 || (t2.baseRoundness ?? 0) > 0) return J(e2, t2);
    let n3 = K(e2, [
      0,
      -t2.height / 2,
      0
    ]), r3 = U([...q(t2.width, t2.depth, t2.height / 2).map((t3) => K(e2, t3)), n3]), i4 = r3.findIndex((e3) => Math.hypot(e3[0] - n3[0], e3[1] - n3[1]) < 0.01);
    if (i4 < 0) return W(r3);
    let a4 = [...r3.slice(i4), ...r3.slice(0, i4)].slice(1);
    return a4.length < 2 ? D2(r3) : `M${n3[0].toFixed(2)} ${n3[1].toFixed(2)}L${a4[0][0].toFixed(2)} ${a4[0][1].toFixed(2)}${fe(a4)}L${n3[0].toFixed(2)} ${n3[1].toFixed(2)}Z`;
  };
  var _e = (e2, t2) => {
    if (t2.roundness > 0) return J(e2, t2);
    let n3 = t2.width / 2, r3 = t2.height / 2, i4 = t2.depth / 2;
    return D2(U([-1, 1].flatMap((e3) => [-1, 1].flatMap((t3) => [-1, 1].map((a4) => [
      e3 * n3,
      t3 * r3,
      a4 * i4
    ]))).map((t3) => K(e2, t3))));
  };
  var ve = (e2, t2) => {
    if (t2.roundness > 0) return J(e2, t2);
    let n3 = t2.width / 2, r3 = t2.height / 2, i4 = t2.depth / 2;
    return D2(U([
      [
        -n3,
        0,
        0
      ],
      [
        n3,
        0,
        0
      ],
      [
        0,
        -r3,
        0
      ],
      [
        0,
        r3,
        0
      ],
      [
        0,
        0,
        -i4
      ],
      [
        0,
        0,
        i4
      ]
    ].map((t3) => K(e2, t3))));
  };
  var Y = (e2, t2, n3, r3, i4) => {
    let a4 = n3 + i4, o4 = Math.hypot(n3 - i4, r3 * 2), s4 = (a4 + o4) / 2, c4 = (a4 - o4) / 2;
    return s4 <= 0 || c4 <= 0 ? null : {
      centerX: e2,
      centerY: t2,
      majorRadius: Math.sqrt(s4),
      minorRadius: Math.sqrt(c4),
      rotation: Math.atan2(r3 * 2, n3 - i4) / 2
    };
  };
  var X = ({ centerX: e2, centerY: t2, majorRadius: n3, minorRadius: r3, rotation: i4 }) => {
    let a4 = i4 * 180 / Math.PI, o4 = Math.cos(i4) * n3, s4 = Math.sin(i4) * n3, c4 = e2 + o4, l4 = t2 + s4, u4 = e2 - o4, d4 = t2 - s4;
    return `M${c4.toFixed(2)} ${l4.toFixed(2)}A${n3.toFixed(2)} ${r3.toFixed(2)} ${a4.toFixed(2)} 0 1 ${u4.toFixed(2)} ${d4.toFixed(2)}A${n3.toFixed(2)} ${r3.toFixed(2)} ${a4.toFixed(2)} 0 1 ${c4.toFixed(2)} ${l4.toFixed(2)}Z`;
  };
  var Z = (e2, t2, n3 = [
    0,
    0,
    0
  ]) => {
    let r3 = [
      y2(e2.orientation, [
        1,
        0,
        0
      ]),
      y2(e2.orientation, [
        0,
        1,
        0
      ]),
      y2(e2.orientation, [
        0,
        0,
        1
      ])
    ], i4 = y2(e2.orientation, n3);
    if (Math.abs(e2.expression.perspective) < 1e-4) {
      let e3 = r3.reduce((e4, n5, r4) => e4 + n5[0] * n5[0] * t2[r4] * t2[r4], 0), n4 = r3.reduce((e4, n5, r4) => e4 + n5[0] * n5[1] * t2[r4] * t2[r4], 0), a4 = r3.reduce((e4, n5, r4) => e4 + n5[1] * n5[1] * t2[r4] * t2[r4], 0);
      return Y(i4[0], i4[1], e3, n4, a4);
    }
    let o4 = t2.map((e3) => 1 / (e3 * e3)), s4 = Array.from({ length: 3 }, (e3, t3) => Array.from({ length: 3 }, (e4, n4) => r3.reduce((e5, r4, i5) => e5 + r4[t3] * o4[i5] * r4[n4], 0))), c4 = a2 / e2.expression.perspective, l4 = [
      -i4[0],
      -i4[1],
      c4 - i4[2]
    ], u4 = [
      s4[0][0] * l4[0] + s4[0][1] * l4[1] + s4[0][2] * l4[2],
      s4[1][0] * l4[0] + s4[1][1] * l4[1] + s4[1][2] * l4[2],
      s4[2][0] * l4[0] + s4[2][1] * l4[1] + s4[2][2] * l4[2]
    ], d4 = l4[0] * u4[0] + l4[1] * u4[1] + l4[2] * u4[2] - 1, f4 = [
      u4[0],
      u4[1],
      -c4 * u4[2]
    ], p4 = [
      [
        s4[0][0],
        s4[0][1],
        -c4 * s4[0][2]
      ],
      [
        s4[1][0],
        s4[1][1],
        -c4 * s4[1][2]
      ],
      [
        -c4 * s4[2][0],
        -c4 * s4[2][1],
        c4 * c4 * s4[2][2]
      ]
    ], m4 = Array.from({ length: 3 }, (e3, t3) => Array.from({ length: 3 }, (e4, n4) => f4[t3] * f4[n4] - d4 * p4[t3][n4])), h4 = m4[0][0] * m4[1][1] - m4[0][1] * m4[0][1];
    if (Math.abs(h4) < 1e-12) return null;
    let g4 = -(m4[1][1] * m4[0][2] - m4[0][1] * m4[1][2]) / h4, _3 = (m4[0][1] * m4[0][2] - m4[0][0] * m4[1][2]) / h4, v3 = -(m4[2][2] + m4[0][2] * g4 + m4[1][2] * _3);
    if (Math.abs(v3) < 1e-12) return null;
    let b3 = m4[0][0] / v3, x3 = m4[0][1] / v3, S3 = m4[1][1] / v3, C3 = b3 * S3 - x3 * x3;
    return C3 <= 0 ? null : Y(g4, _3, S3 / C3, -x3 / C3, b3 / C3);
  };
  var ye = (e2, t2) => {
    let n3 = Z(e2, [
      t2.width / 2,
      t2.height / 2,
      t2.depth / 2
    ]), r3 = t2.width === t2.height && t2.height === t2.depth;
    if (n3 && r3) {
      let e3 = (n3.majorRadius + n3.minorRadius) / 2;
      return X({
        centerX: 0,
        centerY: 0,
        majorRadius: e3,
        minorRadius: e3,
        rotation: 0
      });
    }
    return n3 ? X(n3) : null;
  };
  var be = (e2, t2) => {
    if (t2.type !== "mickey") return [];
    let n3 = Math.min(t2.width, t2.height) * 0.23, r3 = Math.min(n3, t2.depth * 0.29), i4 = t2.width * 0.37, a4 = -t2.height * 0.39, o4 = -t2.depth * 0.12, s4 = [
      n3,
      n3,
      r3
    ];
    return [-1, 1].map((t3) => Z(e2, s4, [
      t3 * i4,
      a4,
      o4
    ])).filter((e3) => e3 !== null).map(X);
  };
  var xe = (e2, t2) => t2.type === "mickey" ? be(e2, t2) : t2.type === "cursor" ? [he(e2, t2)] : [];
  var Q = (e2) => Array.from({ length: k2 }, (t2, n3) => {
    let r3 = n3 / k2 * Math.PI * 2, i4 = Math.cos(r3) * e2.majorRadius, a4 = Math.sin(r3) * e2.minorRadius;
    return [
      e2.centerX + i4 * Math.cos(e2.rotation) - a4 * Math.sin(e2.rotation),
      e2.centerY + i4 * Math.sin(e2.rotation) + a4 * Math.cos(e2.rotation),
      0
    ];
  });
  var Se = (e2) => {
    if (e2.length < 3) return D2(e2);
    let t2 = e2.map((t3, n4) => {
      let r4 = e2[(n4 + 1) % e2.length];
      return Math.hypot(r4[0] - t3[0], r4[1] - t3[1]);
    }), n3 = [...t2].sort((e3, t3) => e3 - t3), r3 = n3[Math.floor(n3.length / 2)] || 1, i4 = Math.max(8, r3 * 3.5), a4 = t2.map((e3) => e3 > i4);
    return `M${e2[0][0].toFixed(2)} ${e2[0][1].toFixed(2)}${e2.map((t3, n4) => {
      let r4 = (n4 + 1) % e2.length, i5 = e2[r4];
      if (a4[n4]) return `L${i5[0].toFixed(2)} ${i5[1].toFixed(2)}`;
      let o4 = a4[(n4 - 1 + e2.length) % e2.length] ? t3 : e2[(n4 - 1 + e2.length) % e2.length], s4 = a4[r4] ? i5 : e2[(n4 + 2) % e2.length], c4 = t3[0] + (i5[0] - o4[0]) / 6, l4 = t3[1] + (i5[1] - o4[1]) / 6, u4 = i5[0] - (s4[0] - t3[0]) / 6, d4 = i5[1] - (s4[1] - t3[1]) / 6;
      return `C${c4.toFixed(2)} ${l4.toFixed(2)} ${u4.toFixed(2)} ${d4.toFixed(2)} ${i5[0].toFixed(2)} ${i5[1].toFixed(2)}`;
    }).join("")}Z`;
  };
  var Ce = (e2, t2) => {
    let n3 = t2.width / 2, r3 = Math.min(n3, t2.height / 2), i4 = t2.depth / 2, a4 = Math.max(0, (t2.height - r3 * 2) / 2), o4 = [
      n3,
      r3,
      i4
    ], s4 = Z(e2, o4, [
      0,
      a4,
      0
    ]), c4 = Z(e2, o4, [
      0,
      -a4,
      0
    ]);
    return !s4 || !c4 ? null : Se(U([...Q(s4), ...Q(c4)]));
  };
  var $ = (e2, t2) => {
    if (t2.type === "sphere" || t2.type === "mickey") {
      let n3 = ye(e2, t2);
      if (n3) return n3;
    }
    if (t2.type === "capsule") {
      let n3 = Ce(e2, t2);
      if (n3) return n3;
    }
    if (t2.type === "cylinder") return pe(e2, t2);
    if (t2.type === "cursor") return me(e2, t2);
    if (t2.type === "cone") return ge(e2, t2);
    if (t2.type === "cube") return _e(e2, t2);
    if (t2.type === "diamond") return ve(e2, t2);
    let r3 = N2(t2), i4 = A2.get(r3);
    return i4 || (i4 = Array.from({ length: se }, (e3, r4) => {
      let i5 = -Math.PI / 2 + r4 / 24 * Math.PI;
      return Array.from({ length: ce }, (e4, r5) => {
        let a4 = -Math.PI + r5 / 72 * Math.PI * 2;
        return T(t2, a4, i5);
      });
    }).flat(), P2(A2, r3, i4)), D2(U(i4.map((t3) => x2(y2(e2.orientation, t3), e2.expression.perspective))));
  };
  var we = (e2, t2) => {
    let r3 = N2(t2.surface), i4 = j2.get(r3);
    i4 || (i4 = Array.from({ length: 17 }, (e3, r4) => {
      let i5 = -Math.PI / 2 + r4 / 16 * Math.PI;
      return Array.from({ length: 49 }, (e4, r5) => {
        let a5 = -Math.PI + r5 / 48 * Math.PI * 2;
        return T(t2.surface, a5, i5);
      });
    }).flat(), P2(j2, r3, i4));
    let a4 = p2(l3(t2.rotation[0]), l3(t2.rotation[1]), l3(t2.rotation[2])), o4 = U(i4.map((n3) => {
      let r4 = y2(a4, n3), i5 = [
        r4[0] + t2.position[0],
        r4[1] + t2.position[1],
        r4[2] + t2.position[2]
      ];
      return x2(y2(e2.orientation, i5), e2.expression.perspective);
    }));
    return (t2.surface.type === "cube" || t2.surface.type === "diamond") && t2.surface.roundness <= 0 ? D2(o4) : W(G(o4));
  };
  var Te = 0.1;
  var Ee = (e2, t2) => {
    let n3 = p2(l3(t2.rotation[0]), l3(t2.rotation[1]), l3(t2.rotation[2])), r3 = [
      [
        1,
        0,
        0
      ],
      [
        0,
        1,
        0
      ],
      [
        0,
        0,
        1
      ]
    ].map((t3) => y2(e2.orientation, y2(n3, t3))[2]);
    return Math.hypot(r3[0] * (t2.surface.width / 2), r3[1] * (t2.surface.height / 2), r3[2] * (t2.surface.depth / 2));
  };
  var De = (e2, t2) => {
    let n3 = t2.map((t3) => {
      let n4 = y2(e2.orientation, t3.position)[2];
      return {
        id: t3.id,
        path: we(e2, t3),
        depth: n4,
        front: n4 > Ee(e2, t3) * Te
      };
    }).sort((e3, t3) => e3.depth - t3.depth);
    return {
      backPaths: n3.filter((e3) => !e3.front).map((e3) => e3.path),
      frontPaths: n3.filter((e3) => e3.front).map((e3) => e3.path),
      backNodeIds: n3.filter((e3) => !e3.front).map((e3) => e3.id),
      frontNodeIds: n3.filter((e3) => e3.front).map((e3) => e3.id)
    };
  };
  var Oe = (e2, t2, n3 = 1, r3 = {}) => {
    let i4 = R(e2, t2, -1, n3, r3.eyeOffset), a4 = R(e2, t2, 1, n3, r3.eyeOffset), o4 = i4.map((e3) => e3.point), s4 = a4.map((e3) => e3.point), c4 = De(e2, r3.bodyNodes ?? []), l4 = xe(e2, t2);
    return {
      backPaths: [...l4, ...c4.backPaths],
      frontPaths: c4.frontPaths,
      backNodeIds: [...l4.map(() => null), ...c4.backNodeIds],
      frontNodeIds: c4.frontNodeIds,
      headPath: $(e2, t2),
      leftPath: D2(o4),
      rightPath: D2(s4),
      leftVisible: i4.reduce((e3, t3) => e3 + t3.normal[2], 0) > 0,
      rightVisible: a4.reduce((e3, t3) => e3 + t3.normal[2], 0) > 0,
      wirePaths: r3.includeWire === false ? [] : B(e2, t2)
    };
  };

  // ajv-stub/dist/2020.js
  var __default = class {
    compile() {
      return () => {
        throw new Error("avatar-core validation is not bundled in notch-hud");
      };
    }
  };

  // pkg/package/dist/index.js
  var k3 = {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $id: "https://avatars.bible-strong.app/schemas/avatar-definition-v1.json",
    title: "Bible Strong Avatar Definition v1",
    type: "object",
    additionalProperties: false,
    required: [
      "schema",
      "schemaVersion",
      "body",
      "colors",
      "expressions",
      "expressionOrder",
      "animations",
      "animationOrder"
    ],
    properties: {
      schema: { const: "bible-strong/avatar-definition" },
      schemaVersion: { const: 1 },
      name: {
        type: "string",
        maxLength: 120
      },
      body: { $ref: "#/$defs/body" },
      colors: { $ref: "#/$defs/colors" },
      expressions: {
        type: "object",
        minProperties: 1,
        maxProperties: 128,
        required: ["neutral"],
        propertyNames: { $ref: "#/$defs/semanticKey" },
        properties: { neutral: { $ref: "#/$defs/expression" } },
        additionalProperties: { $ref: "#/$defs/expression" }
      },
      expressionOrder: {
        type: "array",
        minItems: 1,
        maxItems: 128,
        uniqueItems: true,
        items: { $ref: "#/$defs/semanticKey" }
      },
      animations: {
        type: "object",
        maxProperties: 64,
        propertyNames: { $ref: "#/$defs/semanticKey" },
        additionalProperties: { $ref: "#/$defs/animation" }
      },
      animationOrder: {
        type: "array",
        maxItems: 64,
        uniqueItems: true,
        items: { $ref: "#/$defs/semanticKey" }
      },
      standardAnimationSet: {
        const: 1,
        deprecated: true
      }
    },
    $defs: {
      semanticKey: {
        type: "string",
        maxLength: 64,
        pattern: "^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$"
      },
      hexColor: {
        type: "string",
        pattern: "^#[0-9a-f]{6}$"
      },
      boundedNumber: {
        type: "number",
        minimum: -1e4,
        maximum: 1e4
      },
      dimension: {
        type: "number",
        minimum: 1e-3,
        maximum: 1e4
      },
      roundness: {
        type: "number",
        minimum: 0,
        maximum: 1
      },
      surface: {
        type: "object",
        additionalProperties: false,
        required: [
          "type",
          "width",
          "height",
          "depth",
          "roundness"
        ],
        properties: {
          type: { enum: [
            "sphere",
            "mickey",
            "cursor",
            "cube",
            "capsule",
            "cylinder",
            "cone",
            "diamond"
          ] },
          width: { $ref: "#/$defs/dimension" },
          height: { $ref: "#/$defs/dimension" },
          depth: { $ref: "#/$defs/dimension" },
          roundness: { $ref: "#/$defs/roundness" },
          morphRoundness: { $ref: "#/$defs/roundness" },
          tipRoundness: { $ref: "#/$defs/roundness" },
          baseRoundness: { $ref: "#/$defs/roundness" }
        }
      },
      nodeSurface: { allOf: [{ $ref: "#/$defs/surface" }, {
        type: "object",
        properties: { type: { enum: [
          "sphere",
          "cube",
          "capsule",
          "cylinder",
          "cone",
          "diamond"
        ] } }
      }] },
      position: {
        type: "array",
        minItems: 3,
        maxItems: 3,
        prefixItems: [
          { $ref: "#/$defs/boundedNumber" },
          { $ref: "#/$defs/boundedNumber" },
          { $ref: "#/$defs/boundedNumber" }
        ]
      },
      rotation: {
        type: "array",
        minItems: 3,
        maxItems: 3,
        prefixItems: [
          {
            type: "number",
            minimum: -360,
            maximum: 360
          },
          {
            type: "number",
            minimum: -360,
            maximum: 360
          },
          {
            type: "number",
            minimum: -360,
            maximum: 360
          }
        ]
      },
      body: {
        type: "object",
        additionalProperties: false,
        required: ["primary", "nodes"],
        properties: {
          primary: { $ref: "#/$defs/surface" },
          nodes: {
            type: "array",
            maxItems: 16,
            items: {
              type: "object",
              additionalProperties: false,
              required: [
                "surface",
                "position",
                "rotation"
              ],
              properties: {
                surface: { $ref: "#/$defs/nodeSurface" },
                position: { $ref: "#/$defs/position" },
                rotation: { $ref: "#/$defs/rotation" }
              }
            }
          }
        }
      },
      colors: {
        type: "object",
        additionalProperties: false,
        required: ["body", "eyes"],
        properties: {
          body: { $ref: "#/$defs/hexColor" },
          eyes: { $ref: "#/$defs/hexColor" }
        }
      },
      eye: {
        type: "object",
        additionalProperties: false,
        required: [
          "width",
          "height",
          "x",
          "y",
          "angle"
        ],
        properties: {
          width: { $ref: "#/$defs/boundedNumber" },
          height: { $ref: "#/$defs/boundedNumber" },
          x: { $ref: "#/$defs/boundedNumber" },
          y: { $ref: "#/$defs/boundedNumber" },
          angle: { $ref: "#/$defs/boundedNumber" }
        }
      },
      expression: {
        type: "object",
        additionalProperties: false,
        required: [
          "head",
          "eyes",
          "perspective",
          "motion"
        ],
        properties: {
          head: {
            type: "object",
            additionalProperties: false,
            required: [
              "x",
              "y",
              "z"
            ],
            properties: {
              x: { $ref: "#/$defs/boundedNumber" },
              y: { $ref: "#/$defs/boundedNumber" },
              z: { $ref: "#/$defs/boundedNumber" }
            }
          },
          eyes: {
            type: "object",
            additionalProperties: false,
            required: [
              "left",
              "right",
              "spacing"
            ],
            properties: {
              left: { $ref: "#/$defs/eye" },
              right: { $ref: "#/$defs/eye" },
              spacing: { $ref: "#/$defs/boundedNumber" }
            }
          },
          perspective: {
            type: "number",
            minimum: 0.1,
            maximum: 10
          },
          motion: {
            type: "object",
            additionalProperties: false,
            required: ["eyes", "body"],
            properties: {
              eyes: { enum: [
                "none",
                "microSaccades",
                "shake"
              ] },
              body: { enum: [
                "none",
                "slowDrift",
                "shake"
              ] }
            }
          },
          colors: {
            type: "object",
            additionalProperties: false,
            minProperties: 1,
            properties: {
              body: { $ref: "#/$defs/hexColor" },
              eyes: { $ref: "#/$defs/hexColor" }
            }
          }
        }
      },
      blink: {
        type: "object",
        additionalProperties: false,
        required: [
          "enabled",
          "initialDelayMs",
          "minIntervalMs",
          "maxIntervalMs",
          "durationMs"
        ],
        properties: {
          enabled: { type: "boolean" },
          initialDelayMs: {
            type: "number",
            minimum: 0,
            maximum: 6e4
          },
          minIntervalMs: {
            type: "number",
            minimum: 250,
            maximum: 12e4
          },
          maxIntervalMs: {
            type: "number",
            minimum: 250,
            maximum: 12e4
          },
          durationMs: {
            type: "number",
            minimum: 50,
            maximum: 2e3
          }
        }
      },
      animation: {
        type: "object",
        additionalProperties: false,
        required: [
          "playbackMode",
          "steps",
          "blink"
        ],
        properties: {
          playbackMode: { enum: [
            "loop",
            "once",
            "pingPong"
          ] },
          steps: {
            type: "array",
            minItems: 1,
            maxItems: 128,
            items: {
              type: "object",
              additionalProperties: false,
              required: [
                "expression",
                "holdMs",
                "transitionMs",
                "transition"
              ],
              properties: {
                expression: { $ref: "#/$defs/semanticKey" },
                holdMs: {
                  type: "number",
                  minimum: 100,
                  maximum: 6e4
                },
                transitionMs: {
                  type: "number",
                  minimum: 0,
                  maximum: 5e3
                },
                transition: { enum: [
                  "spring",
                  "smooth",
                  "snappy"
                ] }
              }
            }
          },
          blink: { $ref: "#/$defs/blink" },
          metadata: {
            type: "object",
            additionalProperties: false,
            minProperties: 1,
            properties: {
              label: {
                type: "string",
                maxLength: 120
              },
              description: {
                type: "string",
                maxLength: 512
              },
              group: {
                type: "string",
                maxLength: 64
              }
            }
          }
        }
      }
    }
  };
  var F3 = new __default({
    allErrors: true,
    strict: true
  }).compile(k3);
  var H2 = (e2, t2) => ({
    id: e2,
    semanticKey: e2,
    headX: t2.head.x,
    headY: t2.head.y,
    headZ: t2.head.z,
    widthLeft: t2.eyes.left.width,
    widthRight: t2.eyes.right.width,
    heightLeft: t2.eyes.left.height,
    heightRight: t2.eyes.right.height,
    spacing: t2.eyes.spacing,
    positionXLeft: t2.eyes.left.x,
    positionXRight: t2.eyes.right.x,
    positionYLeft: t2.eyes.left.y,
    positionYRight: t2.eyes.right.y,
    leftAngle: t2.eyes.left.angle,
    rightAngle: t2.eyes.right.angle,
    perspective: t2.perspective,
    eyeMotion: t2.motion.eyes,
    bodyMotion: t2.motion.body,
    ...t2.colors?.body ? { bodyColor: t2.colors.body } : {},
    ...t2.colors?.eyes ? { eyeColor: t2.colors.eyes } : {}
  });
  var U2 = (e2) => ({
    primary: { ...e2.primary },
    nodes: e2.nodes.map((e3, t2) => ({
      id: `runtime-node-${t2}`,
      name: `Runtime node ${t2 + 1}`,
      surface: { ...e3.surface },
      position: [...e3.position],
      rotation: [...e3.rotation]
    }))
  });
  var W2 = (e2, t2, n3 = {}, r3 = 1) => {
    let i4 = U2(e2.body);
    return {
      geometry: Oe(O2(t2), i4.primary, r3, { bodyNodes: i4.nodes }),
      colors: {
        body: n3.body ?? t2.bodyColor ?? e2.colors.body,
        eyes: n3.eyes ?? t2.eyeColor ?? e2.colors.eyes
      }
    };
  };
  var ke = (e2, t2 = "neutral") => {
    let n3 = e2.expressions[t2];
    if (!n3) throw Error(`Unknown expression '${t2}'`);
    return W2(e2, H2(t2, n3), n3.colors);
  };
  var G2 = (e2, t2) => {
    let n3 = e2.animations[t2];
    return n3 ? {
      ok: true,
      value: n3
    } : {
      ok: false,
      error: {
        code: "unknown_animation",
        key: t2,
        message: `Unknown animation '${t2}'`
      }
    };
  };
  var K2 = () => ({
    activeExpression: "neutral",
    status: "stopped",
    stepIndex: 0,
    direction: 1,
    phase: "transition",
    phaseStartedAt: 0,
    transitionFrom: "neutral"
  });
  var je = (e2, t2, n3, r3) => {
    let i4 = G2(e2, t2);
    return i4.ok ? {
      ok: true,
      value: {
        activeAnimation: t2,
        activeExpression: i4.value.steps[0]?.expression ?? "neutral",
        status: "playing",
        stepIndex: 0,
        direction: 1,
        phase: "transition",
        phaseStartedAt: n3,
        transitionFrom: "neutral",
        ...r3 ? { transitionSnapshot: r3 } : {},
        blinkDueAt: n3 + i4.value.blink.initialDelayMs
      }
    } : i4;
  };
  var q2 = (e2, t2) => {
    let n3 = e2.steps.length - 1;
    if (t2.stepIndex < n3 && t2.direction === 1) return {
      stepIndex: t2.stepIndex + 1,
      direction: t2.direction,
      complete: false
    };
    if (t2.stepIndex > 0 && t2.direction === -1) return {
      stepIndex: t2.stepIndex - 1,
      direction: t2.direction,
      complete: false
    };
    if (e2.playbackMode === "once") return {
      stepIndex: t2.stepIndex,
      direction: t2.direction,
      complete: true
    };
    if (e2.playbackMode === "pingPong" && n3 > 0) {
      let e3 = t2.direction === 1 ? -1 : 1;
      return {
        stepIndex: t2.stepIndex + e3,
        direction: e3,
        complete: false
      };
    }
    return {
      stepIndex: 0,
      direction: 1,
      complete: false
    };
  };
  var Me = (e2, t2, n3, r3) => {
    if (t2.directTransition) {
      if (n3 < t2.directTransition.startedAt + t2.directTransition.durationMs) return { ...t2 };
      let { directTransition: e3, ...r4 } = t2;
      return {
        ...r4,
        status: "stopped"
      };
    }
    if (t2.status !== "playing" || !t2.activeAnimation) return { ...t2 };
    let i4 = G2(e2, t2.activeAnimation);
    if (!i4.ok || !i4.value.steps.length) return { ...K2() };
    let a4 = i4.value, o4 = { ...t2 };
    if (a4.blink.enabled && o4.blinkDueAt !== void 0 && n3 >= o4.blinkDueAt) {
      let e3 = o4.blinkDueAt, t3 = a4.blink.minIntervalMs + Math.max(0, Math.min(1, r3.random())) * (a4.blink.maxIntervalMs - a4.blink.minIntervalMs);
      o4.blinkStartedAt = e3, o4.blinkDueAt = e3 + a4.blink.durationMs + t3;
    }
    let s4 = a4.steps.length * 4 + 4;
    for (; s4-- > 0; ) {
      let e3 = a4.steps[o4.stepIndex], t3 = o4.phase === "transition" ? e3.transitionMs : e3.holdMs;
      if (n3 < o4.phaseStartedAt + t3) break;
      if (o4.phaseStartedAt += t3, o4.phase === "transition") {
        o4.phase = "hold", o4.activeExpression = e3.expression;
        continue;
      }
      let r4 = q2(a4, o4);
      if (r4.complete) {
        o4.status = "stopped", delete o4.activeAnimation;
        break;
      }
      o4.stepIndex = r4.stepIndex, o4.direction = r4.direction, o4.phase = "transition", o4.transitionFrom = o4.activeExpression, delete o4.transitionSnapshot, o4.activeExpression = a4.steps[r4.stepIndex].expression;
    }
    return o4;
  };
  var J2 = (e2, t2) => {
    let n3 = Math.max(0, Math.min(1, t2));
    if (e2 === "smooth") return n3 * n3 * (3 - 2 * n3);
    if (e2 === "snappy") return 1 - (1 - n3) ** 3;
    let r3 = 1 - Math.exp(-6) * Math.cos(8);
    return Math.max(0, Math.min(1, (1 - Math.exp(-6 * n3) * Math.cos(8 * n3)) / r3));
  };
  var Y2 = (e2, t2) => ({
    body: t2.colors?.body ?? e2.colors.body,
    eyes: t2.colors?.eyes ?? e2.colors.eyes
  });
  var X2 = (e2, t2, n3) => {
    let r3 = (e3) => {
      let t3 = e3.slice(1), n4 = t3.length === 3 ? [...t3].map((e4) => `${e4}${e4}`).join("") : t3;
      return [
        0,
        2,
        4
      ].map((e4) => Number.parseInt(n4.slice(e4, e4 + 2), 16));
    }, i4 = r3(e2), a4 = r3(t2);
    return i4.some(Number.isNaN) || a4.some(Number.isNaN) ? n3 < 1 ? e2 : t2 : `#${i4.map((e3, t3) => Math.round(e3 + (a4[t3] - e3) * n3)).map((e3) => e3.toString(16).padStart(2, "0")).join("")}`;
  };
  var Z2 = (e2, t2, n3) => ({
    body: X2(e2.body, t2.body, n3),
    eyes: X2(e2.eyes, t2.eyes, n3)
  });
  var Q2 = (e2, t2, n3) => {
    if (!e2.blink.enabled || t2.blinkStartedAt === void 0) return 1;
    let r3 = (n3 - t2.blinkStartedAt) / e2.blink.durationMs;
    return r3 < 0 || r3 >= 1 ? 1 : Math.abs(r3 * 2 - 1);
  };
  var $2 = (e2, t2, n3, r3) => {
    let i4 = t2.status === "paused" && t2.pausedAt !== void 0 ? t2.pausedAt : n3, a4 = e2.expressions[t2.activeExpression];
    if (!a4) {
      let t3 = e2.expressions.neutral;
      return {
        expression: H2("neutral", t3),
        colors: Y2(e2, t3),
        blink: 1,
        sampledAt: i4
      };
    }
    let o4 = H2(t2.activeExpression, a4), s4 = Y2(e2, a4), c4 = 1;
    if (t2.directTransition && !r3.reduceMotion) {
      let e3 = J2(t2.directTransition.transition, (i4 - t2.directTransition.startedAt) / Math.max(t2.directTransition.durationMs, 1));
      o4 = ae(O2(t2.directTransition.from.expression), O2(o4), e3).expression, s4 = Z2(t2.directTransition.from.colors, s4, e3);
    } else if (t2.activeAnimation) {
      let n4 = G2(e2, t2.activeAnimation);
      if (n4.ok) {
        let a5 = n4.value.steps[t2.stepIndex];
        if (t2.phase === "transition" && a5 && !r3.reduceMotion) {
          let n5 = e2.expressions[t2.transitionFrom], r4 = t2.transitionSnapshot?.expression ?? (n5 ? H2(t2.transitionFrom, n5) : void 0), c5 = t2.transitionSnapshot?.colors ?? (n5 ? Y2(e2, n5) : void 0);
          if (r4 && c5) {
            let e3 = Math.max(a5.transitionMs, 1), n6 = J2(a5.transition, (i4 - t2.phaseStartedAt) / e3);
            o4 = ae(O2(r4), O2(o4), n6).expression, s4 = Z2(c5, s4, n6);
          }
        }
        c4 = Q2(n4.value, t2, i4);
      }
    }
    return {
      expression: o4,
      colors: s4,
      blink: c4,
      sampledAt: i4
    };
  };
  var Fe = (e2, t2, n3, i4) => {
    let a4 = $2(e2, t2, n3, i4);
    return W2(e2, i4.reduceMotion ? a4.expression : g(a4.expression, a4.sampledAt), a4.colors, a4.blink);
  };
  return __toCommonJS(entry_exports);
})();
