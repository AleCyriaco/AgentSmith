// Minimal VP8/VP9 decode surface for AgentSmith. This layer only drives libvpx
// and hands back the decoded planes; colour conversion lives in Rust, where it
// can be tested directly.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <vpx/vp8dx.h>
#include <vpx/vpx_decoder.h>

typedef struct {
    vpx_codec_ctx_t codec;
    int ready;
} agentsmith_vpx;

typedef struct {
    int width;
    int height;
    const uint8_t *y;
    const uint8_t *u;
    const uint8_t *v;
    int y_stride;
    int u_stride;
    int v_stride;
    int x_shift;
    int y_shift;
    int full_range;
} agentsmith_vpx_frame;

agentsmith_vpx *agentsmith_vpx_new(int vp9) {
    agentsmith_vpx *decoder = calloc(1, sizeof(agentsmith_vpx));
    if (!decoder) return NULL;
    vpx_codec_dec_cfg_t config;
    memset(&config, 0, sizeof(config));
    config.threads = 4;
    vpx_codec_iface_t *iface = vp9 ? vpx_codec_vp9_dx() : vpx_codec_vp8_dx();
    if (vpx_codec_dec_init(&decoder->codec, iface, &config, 0) != VPX_CODEC_OK) {
        free(decoder);
        return NULL;
    }
    decoder->ready = 1;
    return decoder;
}

void agentsmith_vpx_free(agentsmith_vpx *decoder) {
    if (!decoder) return;
    if (decoder->ready) vpx_codec_destroy(&decoder->codec);
    free(decoder);
}

// 1 when `out` describes planes owned by the decoder and valid until its next
// call; 0 when the frame decoded but has nothing to show, which VP9 does
// routinely for its invisible reference frames; -1 when the frame could not be
// decoded at all. Callers must not treat 0 as a failure: asking the machine for
// a key frame on every invisible frame restarts its encoder over and over.
int agentsmith_vpx_decode(agentsmith_vpx *decoder, const uint8_t *data, size_t length,
                          agentsmith_vpx_frame *out) {
    if (!decoder || !decoder->ready || !out || !data || !length || length > 0x7FFFFFFF) return -1;
    if (vpx_codec_decode(&decoder->codec, data, (unsigned int)length, NULL, 0) != VPX_CODEC_OK)
        return -1;
    vpx_codec_iter_t iterator = NULL;
    const vpx_image_t *image = vpx_codec_get_frame(&decoder->codec, &iterator);
    if (!image) return 0;
    if (!image->d_w || !image->d_h) return -1;
    if (image->fmt & VPX_IMG_FMT_HIGHBITDEPTH) return -1;
    out->width = (int)image->d_w;
    out->height = (int)image->d_h;
    out->y = image->planes[VPX_PLANE_Y];
    out->u = image->planes[VPX_PLANE_U];
    out->v = image->planes[VPX_PLANE_V];
    out->y_stride = image->stride[VPX_PLANE_Y];
    out->u_stride = image->stride[VPX_PLANE_U];
    out->v_stride = image->stride[VPX_PLANE_V];
    out->x_shift = (int)image->x_chroma_shift;
    out->y_shift = (int)image->y_chroma_shift;
    out->full_range = image->range == VPX_CR_FULL_RANGE;
    return (out->y && out->u && out->v) ? 1 : -1;
}
