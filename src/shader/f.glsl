uniform sampler2D t_Tex;
varying vec2 v_Uv;

void main() {
    gl_FragColor = texture2D(t_Tex, v_Uv);
}
