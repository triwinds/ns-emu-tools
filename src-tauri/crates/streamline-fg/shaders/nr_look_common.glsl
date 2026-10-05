const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
vec3 decode(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), greaterThan(c, vec3(0.04045)));
}
bool finite_rgb(vec3 c) {
    return !any(isnan(c)) && !any(isinf(c));
}
vec3 log_change(vec3 p, vec3 n) {
    return log2(max(decode(clamp(n, 0.0, 1.0)), vec3(1e-6)))
        - log2(max(decode(clamp(p, 0.0, 1.0)), vec3(1e-6)));
}
