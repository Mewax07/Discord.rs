(() => {
    const VERTEX = `#version 300 es
in vec2 position;

void main() {
  gl_Position = vec4(position, 0.0, 1.0);
}
`;

    const FRAGMENT = `#version 300 es
precision highp float;

uniform vec2 uResolution;
uniform float uTime;
uniform float uSpeed;
uniform float uScale;
uniform float uRotation;
uniform float uLayers;
uniform float uWaveAmplitude;
uniform float uWaveFrequency;
uniform float uWaveSpeed;
uniform float uLayerSpeed;
uniform float uTwist;
uniform float uTwistFrequency;
uniform float uTwistSpeed;
uniform float uLineFrequency;
uniform float uLineSpacing;
uniform float uLineSharpness;
uniform float uGlowFalloff;
uniform float uGlowIntensity;
uniform float uBrightness;
uniform float uBlueBoost;
uniform float uVignette;
uniform float uGrain;
uniform float uRotationSpeed;
uniform float uLightMode;
uniform vec3 uLineColor;
uniform vec3 uGlowColor;

out vec4 fragColor;

#define MAX_LAYERS 10

mat2 rotate2d(float angle) {
  float sine = sin(angle);
  float cosine = cos(angle);
  return mat2(cosine, -sine, sine, cosine);
}

float grainHash(vec2 point) {
  point = floor(point);
  float hash = 52.9829189 * fract(dot(point, vec2(0.065, 0.005)));
  return fract(hash);
}

float layeredGrain(vec2 fragmentPixel) {
  vec2 point = mod(fragmentPixel + vec2(uTime * 30.0, -uTime * 21.0), 1024.0);
  vec2 rotated = mat2(0.8, -0.5, 0.5, 0.8) * point;
  float grain = 0.0;
  grain += 0.40 * grainHash(rotated);
  grain += 0.25 * grainHash(rotated * 2.0 + 17.0);
  grain += 0.20 * grainHash(rotated * 4.0 + 47.0);
  grain += 0.10 * grainHash(rotated * 8.0 + 113.0);
  grain += 0.05 * grainHash(rotated * 16.0 + 191.0);
  return grain;
}

void main() {
  vec2 resolution = max(uResolution, vec2(1.0));
  vec2 uv = (2.0 * gl_FragCoord.xy - resolution) / resolution.y;
  float time = uTime * uSpeed;
  vec3 backdrop = mix(vec3(0.070588, 0.058824, 0.090196), vec3(1.0), step(0.5, uLightMode));
  vec3 centerTone = max(uLineColor * 0.85567 - uGlowColor * 0.06186, vec3(0.0));
  vec3 cloudTone = uLineColor * 0.19588 + uGlowColor * 0.2268;
  vec2 p = uv;
  p /= max(uScale, 0.05);
  p = rotate2d(radians(uRotation) + time * uRotationSpeed) * p;
  vec3 color = vec3(0.0);
  float fiberField = 0.0;

  for (int index = 0; index < MAX_LAYERS; index++) {
    float fi = float(index) + 1.0;
    if (fi > uLayers) break;

    p += uWaveAmplitude * sin(p.yx * fi * uWaveFrequency + time * (uWaveSpeed + fi * uLayerSpeed));

    float radius = length(p);
    float polarAngle = atan(p.y, p.x);
    polarAngle += sin(radius * uTwistFrequency - time * uTwistSpeed + fi) * uTwist;
    p = vec2(cos(polarAngle), sin(polarAngle)) * radius;

    float lines = abs(sin(p.x * (uLineFrequency + fi * uLineSpacing) + sin(p.y * 3.0 + time)));
    lines = pow(max(0.0, 1.0 - lines), uLineSharpness);
    fiberField += lines / fi;
    color += uLineColor * lines / fi;

    float glow = exp(-uGlowFalloff * abs(sin(p.x * 3.0 + time + fi)));
    color += uGlowColor * glow * uGlowIntensity / (fi * 2.0);
  }

  float center = exp(-2.2 * dot(uv, uv));
  color += centerTone * center;

  float cloud = exp(-1.5 * length(uv + vec2(sin(time * 0.3) * 0.25, cos(time * 0.25) * 0.18)));
  color += cloudTone * cloud;

  float vignette = 1.0 - smoothstep(0.35, 1.45, length(uv));
  color *= mix(1.0 - uVignette, 1.0, vignette);
  color = 1.0 - exp(-color * uBrightness);
  color.b *= uBlueBoost;

  vec3 outputColor;
  if (uLightMode > 0.5) {
    float edgeFade = mix(1.0 - uVignette, 1.0, vignette);
    float fibers = pow(smoothstep(0.12, 1.05, fiberField) * edgeFade, 1.5);
    float atmosphere = (center * 0.025 + cloud * 0.015) * edgeFade;
    vec3 fiberInk = mix(backdrop, uLineColor, 0.52);
    vec3 airColor = mix(backdrop, uGlowColor, 0.16);

    outputColor = mix(backdrop, airColor, atmosphere);
    outputColor = mix(outputColor, fiberInk, fibers * 0.3);
  } else {
    outputColor = backdrop + color;
  }

  float noise = (layeredGrain(gl_FragCoord.xy) - 0.5) * uGrain;
  outputColor = clamp(outputColor + noise, 0.0, 1.0);
  fragColor = vec4(outputColor, 1.0);
}
`;

    const DEFAULTS = {
        lineColor: "#140E35",
        glowColor: "#3437A0",
        speed: 0.2,
        scale: 2,
        rotation: 0,
        rotationSpeed: 0.25,
        layers: 4,
        waveAmplitude: 0.015,
        waveFrequency: 3,
        waveSpeed: 0.15,
        layerSpeed: 0.08,
        twist: 0.1,
        twistFrequency: 5,
        twistSpeed: 1.2,
        lineFrequency: 5,
        lineSpacing: 2,
        lineSharpness: 16,
        glowFalloff: 10,
        glowIntensity: 1.6,
        brightness: 2,
        blueBoost: 1.25,
        vignette: 0.8,
        grain: 0.05,
        lightMode: false,
        dpr: 1,
        fps: 60,
        paused: false,
    };

    const UNIFORM_MAP = {
        uSpeed: "speed",
        uScale: "scale",
        uRotation: "rotation",
        uRotationSpeed: "rotationSpeed",
        uLayers: "layers",
        uWaveAmplitude: "waveAmplitude",
        uWaveFrequency: "waveFrequency",
        uWaveSpeed: "waveSpeed",
        uLayerSpeed: "layerSpeed",
        uTwist: "twist",
        uTwistFrequency: "twistFrequency",
        uTwistSpeed: "twistSpeed",
        uLineFrequency: "lineFrequency",
        uLineSpacing: "lineSpacing",
        uLineSharpness: "lineSharpness",
        uGlowFalloff: "glowFalloff",
        uGlowIntensity: "glowIntensity",
        uBrightness: "brightness",
        uBlueBoost: "blueBoost",
        uVignette: "vignette",
        uGrain: "grain",
    };

    const hexToRgb = (hex) => {
        const value = String(hex).trim().replace(/^#/, "");
        const normalized =
            value.length === 3
                ? value.replace(/./g, (channel) => channel + channel)
                : value;
        const match = /^([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(
            normalized,
        );
        if (!match) return [1, 1, 1];
        return [
            parseInt(match[1], 16) / 255,
            parseInt(match[2], 16) / 255,
            parseInt(match[3], 16) / 255,
        ];
    };

    const compile = (gl, type, source) => {
        const shader = gl.createShader(type);
        gl.shaderSource(shader, source);
        gl.compileShader(shader);
        if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
            console.error("ghost-fibers", gl.getShaderInfoLog(shader));
            gl.deleteShader(shader);
            return null;
        }
        return shader;
    };

    const link = (gl) => {
        const vertex = compile(gl, gl.VERTEX_SHADER, VERTEX);
        const fragment = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT);
        if (!vertex || !fragment) return null;

        const program = gl.createProgram();
        gl.attachShader(program, vertex);
        gl.attachShader(program, fragment);
        gl.linkProgram(program);
        gl.deleteShader(vertex);
        gl.deleteShader(fragment);

        if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
            console.error("ghost-fibers", gl.getProgramInfoLog(program));
            gl.deleteProgram(program);
            return null;
        }
        return program;
    };

    window.mountGhostFibers = (container, options) => {
        if (!container) return null;

        const settings = Object.assign({}, DEFAULTS, options || {});
        const canvas = document.createElement("canvas");
        canvas.setAttribute("aria-hidden", "true");
        canvas.style.width = "100%";
        canvas.style.height = "100%";
        canvas.style.display = "block";

        const gl = canvas.getContext("webgl2", {
            alpha: false,
            antialias: false,
            depth: false,
            stencil: false,
            powerPreference: "high-performance",
        });

        if (!gl) {
            container.dataset.shader = "unsupported";
            return null;
        }

        const program = link(gl);
        if (!program) {
            container.dataset.shader = "failed";
            return null;
        }

        container.append(canvas);

        const buffer = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        gl.bufferData(
            gl.ARRAY_BUFFER,
            new Float32Array([-1, -1, 3, -1, -1, 3]),
            gl.STATIC_DRAW,
        );

        const vao = gl.createVertexArray();
        gl.bindVertexArray(vao);
        const location = gl.getAttribLocation(program, "position");
        gl.enableVertexAttribArray(location);
        gl.vertexAttribPointer(location, 2, gl.FLOAT, false, 0, 0);

        gl.useProgram(program);

        const uniforms = {};
        const names = [
            "uResolution",
            "uTime",
            "uLightMode",
            "uLineColor",
            "uGlowColor",
            ...Object.keys(UNIFORM_MAP),
        ];
        names.forEach((name) => {
            uniforms[name] = gl.getUniformLocation(program, name);
        });

        const apply = () => {
            gl.useProgram(program);
            Object.entries(UNIFORM_MAP).forEach(([name, key]) => {
                gl.uniform1f(uniforms[name], Number(settings[key]));
            });
            gl.uniform1f(
                uniforms.uLayers,
                Math.min(Math.max(Math.round(settings.layers), 1), 10),
            );
            gl.uniform1f(uniforms.uLightMode, settings.lightMode ? 1 : 0);
            gl.uniform3fv(uniforms.uLineColor, hexToRgb(settings.lineColor));
            gl.uniform3fv(uniforms.uGlowColor, hexToRgb(settings.glowColor));
        };

        let frame = 0;
        let elapsed = 0;
        let previous = performance.now();
        let lastRender = 0;
        let frameRate = Math.min(Math.max(settings.fps, 1), 120);
        let paused = Boolean(settings.paused);
        let onScreen = true;
        let pageVisible = !document.hidden;
        const reducedMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        );

        const draw = () => {
            gl.useProgram(program);
            gl.bindVertexArray(vao);
            gl.uniform1f(uniforms.uTime, elapsed);
            gl.drawArrays(gl.TRIANGLES, 0, 3);
        };

        const stop = () => {
            if (frame !== 0) cancelAnimationFrame(frame);
            frame = 0;
        };

        const canAnimate = () =>
            onScreen &&
            pageVisible &&
            !paused &&
            !reducedMotion.matches &&
            !gl.isContextLost();

        const loop = (now) => {
            frame = 0;
            if (!canAnimate()) return;

            const delta = Math.min((now - previous) / 1000, 0.1);
            previous = now;
            elapsed += delta;

            if (now - lastRender >= 1000 / frameRate - 0.5) {
                draw();
                lastRender = now;
            }

            frame = requestAnimationFrame(loop);
        };

        const start = () => {
            if (!canAnimate() || frame !== 0) return;
            previous = performance.now();
            frame = requestAnimationFrame(loop);
        };

        const resize = () => {
            const rect = container.getBoundingClientRect();
            const ratio = Math.min(Math.max(settings.dpr, 0.5), 2);
            const width = Math.max(1, Math.floor(rect.width * ratio));
            const height = Math.max(1, Math.floor(rect.height * ratio));

            if (canvas.width !== width || canvas.height !== height) {
                canvas.width = width;
                canvas.height = height;
            }

            gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
            gl.useProgram(program);
            gl.uniform2f(
                uniforms.uResolution,
                gl.drawingBufferWidth,
                gl.drawingBufferHeight,
            );
            draw();
        };

        const sync = () => {
            if (canAnimate()) start();
            else {
                stop();
                if (!gl.isContextLost()) draw();
            }
        };

        const onPageVisibility = () => {
            pageVisible = !document.hidden;
            sync();
        };

        const onContextLost = (event) => {
            event.preventDefault();
            stop();
        };

        const resizeObserver = new ResizeObserver(resize);
        resizeObserver.observe(container);

        const intersectionObserver = new IntersectionObserver(
            ([entry]) => {
                onScreen = entry.isIntersecting;
                sync();
            },
            { threshold: 0 },
        );
        intersectionObserver.observe(container);

        document.addEventListener("visibilitychange", onPageVisibility);
        reducedMotion.addEventListener("change", sync);
        canvas.addEventListener("webglcontextlost", onContextLost);

        apply();
        resize();
        start();
        container.dataset.shader = "ready";

        return {
            update(next) {
                Object.assign(settings, next || {});
                frameRate = Math.min(Math.max(settings.fps, 1), 120);
                paused = Boolean(settings.paused);
                apply();
                sync();
                if (!canAnimate()) draw();
            },
            destroy() {
                stop();
                resizeObserver.disconnect();
                intersectionObserver.disconnect();
                document.removeEventListener(
                    "visibilitychange",
                    onPageVisibility,
                );
                reducedMotion.removeEventListener("change", sync);
                canvas.removeEventListener("webglcontextlost", onContextLost);
                gl.deleteProgram(program);
                gl.deleteBuffer(buffer);
                gl.deleteVertexArray(vao);
                if (canvas.parentNode === container)
                    container.removeChild(canvas);
                gl.getExtension("WEBGL_lose_context")?.loseContext();
                delete container.dataset.shader;
            },
        };
    };
})();
