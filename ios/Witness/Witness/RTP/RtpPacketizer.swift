import Foundation
import CoreMedia

/// H.264 over RTP per RFC 6184. Single-NAL mode + FU-A fragmentation.
///
/// All packets of one frame share the same 90kHz timestamp. The RTP marker
/// bit is set only on the last packet of the last NALU of a frame.
final class RtpPacketizer {
    let ssrc: UInt32
    let payloadType: UInt8 = 96
    let clockRate: Int32 = 90000
    var mtu: Int = 1400

    private var seqnum: UInt16

    init(ssrc: UInt32) {
        self.ssrc = ssrc
        self.seqnum = UInt16.random(in: 0...UInt16.max)
    }

    /// Convert a frame's NALUs into one or more RTP packets ready for sendto().
    func packetize(nalus: [Data], pts: CMTime) -> [Data] {
        let scaled = CMTimeConvertScale(pts, timescale: clockRate, method: .default)
        let ts = UInt32(truncatingIfNeeded: scaled.value)

        var packets: [Data] = []
        for (idx, nalu) in nalus.enumerated() {
            guard !nalu.isEmpty else { continue }
            let isLastNalu = (idx == nalus.count - 1)
            if nalu.count <= mtu - 12 {
                packets.append(singleNal(nalu: nalu, ts: ts, marker: isLastNalu))
            } else {
                packets.append(contentsOf: fuA(nalu: nalu, ts: ts, markerOnLast: isLastNalu))
            }
        }
        return packets
    }

    // MARK: - private

    private func makeHeader(ts: UInt32, marker: Bool) -> Data {
        var hdr = Data(count: 12)
        hdr[0] = 0b1000_0000                                         // V=2, P=0, X=0, CC=0
        hdr[1] = (marker ? 0x80 : 0) | (payloadType & 0x7f)          // M | PT
        let seq = seqnum
        seqnum &+= 1
        hdr[2] = UInt8((seq >> 8) & 0xff)
        hdr[3] = UInt8(seq & 0xff)
        hdr[4] = UInt8((ts >> 24) & 0xff)
        hdr[5] = UInt8((ts >> 16) & 0xff)
        hdr[6] = UInt8((ts >> 8) & 0xff)
        hdr[7] = UInt8(ts & 0xff)
        hdr[8] = UInt8((ssrc >> 24) & 0xff)
        hdr[9] = UInt8((ssrc >> 16) & 0xff)
        hdr[10] = UInt8((ssrc >> 8) & 0xff)
        hdr[11] = UInt8(ssrc & 0xff)
        return hdr
    }

    private func singleNal(nalu: Data, ts: UInt32, marker: Bool) -> Data {
        var pkt = makeHeader(ts: ts, marker: marker)
        pkt.append(nalu)
        return pkt
    }

    private func fuA(nalu: Data, ts: UInt32, markerOnLast: Bool) -> [Data] {
        // FU-A: split a NALU bigger than the MTU into multiple RTP packets.
        // First byte of original NALU is the header — broken out into:
        //   FU indicator byte = F | NRI | Type=28
        //   FU header byte   = S | E | R | original NALU type
        let header = nalu[nalu.startIndex]
        let f   = header & 0x80
        let nri = header & 0x60
        let originalType = header & 0x1f
        let body = nalu.dropFirst()

        let fragMax = mtu - 12 - 2                                    // RTP hdr + FU ind + FU hdr
        var packets: [Data] = []
        var offset = 0
        var isFirstFragment = true

        while offset < body.count {
            let remaining = body.count - offset
            let chunk = min(fragMax, remaining)
            let isLastFragment = (offset + chunk == body.count)
            let marker = isLastFragment && markerOnLast

            var pkt = makeHeader(ts: ts, marker: marker)
            pkt.append(f | nri | 28)                                  // FU indicator (28 = FU-A)
            var fuHdr: UInt8 = originalType
            if isFirstFragment { fuHdr |= 0x80 }                       // S start
            if isLastFragment  { fuHdr |= 0x40 }                       // E end
            pkt.append(fuHdr)

            let start = body.index(body.startIndex, offsetBy: offset)
            let end   = body.index(start, offsetBy: chunk)
            pkt.append(body[start..<end])

            packets.append(pkt)
            offset += chunk
            isFirstFragment = false
        }
        return packets
    }
}
