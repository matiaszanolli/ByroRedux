/* Keep FFmpeg struct access on the C side of the wrapper's private ABI. */
#include <errno.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>

/* A WMA v2 superframe contains at most 15 codec frames of 2,048 PCM frames each. */
enum { MAX_SUPERFRAME_FRAMES = 15 * 2048 };

AVCodecContext *byro_xwma_context_alloc(void)
{
    const AVCodec *codec = avcodec_find_decoder(AV_CODEC_ID_WMAV2);
    return codec ? avcodec_alloc_context3(codec) : NULL;
}

int byro_xwma_context_open(AVCodecContext *ctx, int channels, int rate,
                          int avg_bytes, int align)
{
    ctx->codec_tag = 0x0161;
    ctx->sample_rate = rate;
    ctx->pkt_timebase = (AVRational){ 1, rate };
    ctx->bit_rate = 8LL * avg_bytes;
    ctx->block_align = align;
    ctx->bits_per_coded_sample = 16;
    ctx->thread_count = 1;
    ctx->max_samples = MAX_SUPERFRAME_FRAMES * channels;
    av_channel_layout_default(&ctx->ch_layout, channels);

    /* Match the xWMA demuxer. Empty extradata disables the bit reservoir. */
    ctx->extradata = av_mallocz(6 + AV_INPUT_BUFFER_PADDING_SIZE);
    if (!ctx->extradata)
        return AVERROR(ENOMEM);
    ctx->extradata_size = 6;
    ctx->extradata[4] = 0x1f;
    return avcodec_open2(ctx, NULL, NULL);
}

int byro_xwma_send(AVCodecContext *ctx, AVPacket *pkt,
                  const unsigned char *bytes, int len)
{
    if (len <= 0 || len > ctx->block_align)
        return AVERROR(EINVAL);
    /* av_new_packet supplies the zeroed FFmpeg bitreader overread padding. */
    int rc = av_new_packet(pkt, ctx->block_align);
    if (rc < 0)
        return rc;
    memcpy(pkt->data, bytes, len);
    memset(pkt->data + len, 0, ctx->block_align - len);
    rc = avcodec_send_packet(ctx, pkt);
    av_packet_unref(pkt);
    return rc;
}

/* Positive results distinguish the two normal non-frame returns from errors. */
int byro_xwma_receive(AVCodecContext *ctx, AVFrame *frame, int channels,
                     int rate, int *frames)
{
    int rc = avcodec_receive_frame(ctx, frame);
    if (rc == AVERROR(EAGAIN))
        return 1;
    if (rc == AVERROR_EOF)
        return 2;
    if (rc < 0)
        return rc;

    if (frame->format != AV_SAMPLE_FMT_FLTP ||
        frame->ch_layout.nb_channels != channels || frame->sample_rate != rate ||
        frame->nb_samples <= 0 || frame->nb_samples > MAX_SUPERFRAME_FRAMES ||
        frame->linesize[0] < frame->nb_samples * (int)sizeof(float) ||
        !frame->extended_data)
        return AVERROR_INVALIDDATA;
    for (int ch = 0; ch < channels; ++ch) {
        if (!frame->extended_data[ch])
            return AVERROR_INVALIDDATA;
    }

    *frames = frame->nb_samples;
    return 0;
}

/* The caller holds the validated frame and provides room for frames * channels. */
void byro_xwma_copy(AVFrame *frame, int offset, int frames, float *out)
{
    const float *left = (const float *)frame->extended_data[0] + offset;
    if (frame->ch_layout.nb_channels == 1) {
        memcpy(out, left, (size_t)frames * sizeof(float));
    } else {
        const float *right = (const float *)frame->extended_data[1] + offset;
        for (int i = 0; i < frames; ++i) {
            *out++ = left[i];
            *out++ = right[i];
        }
    }
    if (offset + frames == frame->nb_samples)
        av_frame_unref(frame);
}
