cbuffer Uniforms : register(b0) {
    float2 viewSize;
    float scale;
    float time;
    float columnWidth;
    int columnCount;
    float pileTop;
    int occluderCount;
    float2 origin;
    float pileBottom;
    int heightOffset;
    float4 light;
    float4 shadowTint;
    float4 occluders[16];
    // x: opacity of everything drawn; y: how far fast particles smear, relative to snow; z: how much the drifts glitter.
    float4 look;
    float4 pileTint;
};

Buffer<float> heights : register(t0);

struct SpriteIn {
    float2 position : POSITION;
    float2 velocity : VELOCITY;
    float size : SIZE;
    float alpha : ALPHA;
    float soft : SOFT;
    float tintAmount : TINTAMOUNT;
    float3 tintColor : TINTCOLOR;
    float lump : LUMP;
    float shape : SHAPE;
    uint vid : SV_VertexID;
};

struct SpriteOut {
    float4 position : SV_Position;
    float2 coord : TEXCOORD0;
    float alpha : ALPHA;
    float soft : SOFT;
    float4 tint : TINT;
    float2 streak : STREAK;
    float radius : RADIUS;
    float lump : LUMP;
    float spin : SPIN;
    float shape : SHAPE;
};

SpriteOut spriteVertex(SpriteIn s) {
    float pixels = s.size * scale;
    // About a 1/55 s exposure, capped so even the fastest grain stays a short smear and never reads as rain, unless it is rain.
    float2 travel = s.velocity * 0.018;
    // Rain's long streaks are for the drops only; leaves, petals and stars are drawn as they are, without a smear.
    float streakScale = (s.lump > 0.0 || s.tintAmount >= 1.0) ? 1.0 : look.y;
    float smear = s.shape > 0.5 ? 0.0 : min(length(travel), s.size * (s.lump > 0.0 ? 1.2 : 2.2) * streakScale);
    float2 streak = smear > 0.25 ? normalize(travel) * smear * scale : float2(0.0, 0.0);
    // A lump's ragged edge reaches past its nominal size.
    float reach = s.lump > 0.0 ? 1.3 : 1.0;
    float extent = max(pixels * reach + length(streak), 2.0);
    float2 corner = float2(float(s.vid & 1), float(s.vid >> 1));
    SpriteOut o;
    float2 center = s.position / viewSize * 2.0 - 1.0;
    o.position = float4(center + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
    o.coord = float2(corner.x, 1.0 - corner.y);
    o.streak = streak / extent;
    o.radius = pixels / extent * 0.5;
    // A streak spreads the same flake over more pixels, so each is fainter.
    o.alpha = s.alpha * min(pixels / 2.0, 1.0) * pixels / max(pixels + length(streak) * 0.6, 0.001);
    o.soft = s.soft;
    o.tint = float4(s.tintColor, s.tintAmount);
    o.lump = s.lump;
    // Leaves and petals turn slowly as they drift; lumps of snow tumble faster.
    o.spin = s.lump > 0.0 ? time * (s.lump - 0.5) * (s.shape > 0.5 ? 3.0 : 9.0) + s.lump * 40.0 : 0.0;
    o.shape = s.shape;
    return o;
}

float4 spriteFragment(SpriteOut i) : SV_Target {
    // Quad coordinates run down the screen; velocities run up.
    float2 p = float2(i.coord.x - 0.5, 0.5 - i.coord.y);
    float2 half_ = i.streak * 0.5;
    float span = dot(half_, half_);
    float t = span > 0.0 ? clamp(dot(p, half_) / span, -1.0, 1.0) : 0.0;
    float2 d = p - half_ * t;
    float r = length(d) / max(i.radius, 0.001);
    if (i.shape > 0.5) {
        // In the sprite's own turning frame, with the shape filling a unit circle.
        float2 q = d / max(i.radius, 0.001);
        float cs = cos(i.spin);
        float sn = sin(i.spin);
        q = float2(q.x * cs - q.y * sn, q.x * sn + q.y * cs);
        float cover = 0.0;
        float3 tone = i.tint.rgb;
        if (i.shape < 1.5) {
            // A leaf: a pointed oval with a darker midrib, narrowing as it turns edge-on.
            float flip = 0.3 + 0.7 * abs(cos(i.spin * 0.7 + i.lump * 13.0));
            float halfWidth = 0.44 * flip * pow(saturate(1.0 - q.x * q.x), 0.7);
            cover = (1.0 - smoothstep(-0.05, 0.03, abs(q.y) - halfWidth)) * (1.0 - smoothstep(0.9, 1.0, abs(q.x)));
            float rib = (1.0 - smoothstep(0.0, 0.06, abs(q.y))) * step(-0.9, q.x);
            float veins = (1.0 - smoothstep(0.0, 0.05, abs(frac((q.x + abs(q.y) * 0.9) * 3.0) - 0.5) - 0.42)) * 0.12;
            tone *= (0.78 + 0.32 * (q.x * 0.5 + 0.5)) * (1.0 - 0.3 * rib - veins) * (0.8 + 0.3 * flip);
        } else if (i.shape < 2.5) {
            // A petal: a rounded teardrop, paler at its base, cupped as it turns.
            float flip = 0.45 + 0.55 * abs(cos(i.spin * 0.6 + i.lump * 7.0));
            float halfWidth = 0.52 * flip * sqrt(saturate(1.0 - q.x * q.x)) * (0.5 + 0.5 * saturate(q.x * 0.5 + 0.6));
            cover = (1.0 - smoothstep(-0.08, 0.03, abs(q.y) - halfWidth)) * (1.0 - smoothstep(0.88, 1.0, abs(q.x)));
            tone = lerp(float3(1.0, 0.97, 0.98), tone, saturate(0.45 + 0.55 * (q.x * 0.5 + 0.5))) * (0.88 + 0.12 * flip);
        } else {
            // A star: a bright core with four soft rays.
            float rays = exp(-abs(q.x) * 12.0) * exp(-abs(q.y) * 2.6) + exp(-abs(q.y) * 12.0) * exp(-abs(q.x) * 2.6);
            cover = saturate(rays + exp(-dot(q, q) * 16.0));
            tone = lerp(float3(1.0, 1.0, 1.0), tone, 0.45);
        }
        float shapeAlpha = cover * i.alpha * look.x;
        if (shapeAlpha < 0.002) discard;
        return float4(tone * light.rgb * shapeAlpha, shapeAlpha);
    }
    float shading = 0.0;
    if (i.lump > 0.0) {
        float angle = atan2(d.y, d.x) + i.spin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + i.lump * 21.0) + 0.05 * sin(2.0 * angle + i.lump * 9.0);
        shading = saturate(dot(d / max(i.radius, 0.001), float2(0.45, -0.6))) * 0.4;
    }
    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(lerp(0.6, 0.0, i.soft), 1.0, r)) * i.alpha * head;
    if (a < 0.002) discard;
    float3 core = float3(0.965, 0.975, 0.995);
    float3 rim = float3(0.62, 0.67, 0.77);
    float3 color = lerp(core, rim, saturate(smoothstep(0.35, 1.0, r) * 0.65 + shading));
    color = lerp(color, i.tint.rgb, i.tint.a) * light.rgb;
    a *= look.x;
    return float4(color * a, a);
}

struct PileOut {
    float4 position : SV_Position;
    float2 local : TEXCOORD0;
};

PileOut pileVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 p = float2(corner.x * float(columnCount) * columnWidth, lerp(pileBottom, pileTop, corner.y));
    PileOut o;
    o.position = float4((p + origin) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    o.local = p;
    return o;
}

float heightAt(int start, float x) {
    float f = x / columnWidth - 0.5;
    float base = floor(f);
    int i = clamp(int(base), 0, columnCount - 1);
    int j = min(i + 1, columnCount - 1);
    return lerp(heights[start + i], heights[start + j], saturate(f - base));
}

float hash(float2 p) {
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

float4 pileFragment(PileOut i) : SV_Target {
    float2 world = i.local + origin;
    [loop]
    for (int k = 0; k < occluderCount; k++) {
        float4 o = occluders[k];
        if (world.x >= o.x && world.x < o.z && world.y >= o.y && world.y < o.w) discard;
    }

    float x = i.local.x;
    float level = heightAt(heightOffset, x);
    float bed = heightAt(heightOffset + columnCount, x);
    float depth = level - i.local.y;
    float coverage = saturate(depth * scale + 0.5)
        * saturate((i.local.y - bed) * scale + 0.5)
        * saturate((level - bed) / 0.75);
    if (coverage <= 0.0) discard;

    float slope = (heightAt(heightOffset, x + 2.0) - heightAt(heightOffset, x - 2.0)) * 0.25;
    float lit = dot(normalize(float2(-slope, 1.0)), normalize(float2(-0.5, 1.0)));

    float3 shadow = lerp(float3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
    float3 surface = lerp(float3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, float3(1.0, 1.0, 1.0)), shadowTint.a * 0.2);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0);
    float3 color = lerp(surface, shadow, saturate(shade));
    color = lerp(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
    color += (hash(floor(i.position.xy)) - 0.5) * 0.02;

    float seed = hash(floor(i.position.xy * 0.5) + 31.7);
    if (seed > 1.0 - 0.008 * look.z) {
        float twinkle = pow(saturate(sin(time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
        color = lerp(color, float3(1.0, 1.0, 1.0), twinkle * exp(-depth / 7.0));
    }

    // Leaves and petals lie as a mottled heap of slightly different colours, not one flat one.
    float2 cell = floor(i.position.xy / (3.0 * scale));
    float speckle = hash(cell + 3.1);
    float3 varied = pileTint.rgb * (0.72 + 0.56 * speckle);
    varied = lerp(varied, varied.gbr, 0.18 * hash(cell + 17.0));
    color = lerp(color, color * varied * 1.1, pileTint.a);
    color *= light.rgb;
    coverage *= look.x;
    return float4(color * coverage, coverage);
}
