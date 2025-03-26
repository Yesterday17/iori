FROM=4450
TO=6932

echo '#EXTM3U
#EXT-X-KEY:METHOD=SAMPLE-AES,URI="skd://streaks?assetId=81cdffc5ef204bbc9684a20339b2f9f9-1&variantId=&keyId=3c72de695d81445e9a625ff6d8dd19a0&keyRotationId=0",IV=0x003753819E159B549F1B86AEB8F63B9F,KEYFORMAT="com.apple.streamingkeydelivery",KEYFORMATVERSIONS="1"'

for i in $(seq $FROM $TO); do
    echo '#EXTINF:6.006,'
    echo "https://live7.happyon-cdn.jp/4c7d7bf21676417089d5d101a783744d/145122b99006494a905b3a50bbe09b16/manifest_6_$i.ts"
done

echo '#EXT-X-ENDLIST'