import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile
from xml.etree import ElementTree as ET

from common import NS

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'output/pptx-benchmark-20261106/timing-probes'
P14 = 'http://schemas.microsoft.com/office/powerpoint/2010/main'
MORPH = 'http://schemas.microsoft.com/office/powerpoint/2015/09/main'
MC = 'http://schemas.openxmlformats.org/markup-compatibility/2006'


def main():
    global OUT
    if len(sys.argv) > 2: OUT = OUT / sys.argv[2]
    OUT.mkdir(exist_ok=True, parents=True)
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark')
    def ctime(ident):
        return f'<p:cTn id="{ident}" dur="500"/>'
    def behavior(ident, target='<p:spTgt spid="7"/>'):
        return f'<p:cBhvr additive="sum" accumulate="always" xfrmType="img" from="a" to="b" by="c" rctx="ctx" override="childStyle">{ctime(ident)}<p:tgtEl>{target}</p:tgtEl><p:attrNameLst><p:attrName>style.opacity</p:attrName></p:attrNameLst></p:cBhvr>'
    operations = [
        '<p:par>'+ctime(2)+'</p:par>',
        '<p:seq concurrent="true" prevAc="skipTimed" nextAc="seek">'+ctime(3)+'<p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="indefinite"><p:tn val="2"/></p:cond></p:nextCondLst></p:seq>',
        '<p:excl>'+ctime(4)+'</p:excl>',
        '<p:anim calcmode="lin" valueType="num" from="0" to="1" by=".1">'+behavior(5)+'<p:tavLst><p:tav tm="25000" fmla="x*2"><p:val><p:fltVal val="0.25"/></p:val></p:tav><p:tav tm="100%"><p:val><p:strVal val="A &amp; B"/></p:val></p:tav></p:tavLst></p:anim>',
        '<p:animClr clrSpc="hsl" dir="ccw">'+behavior(6)+'<p:by><p:hsl h="5400000" s="20000" l="-10000"/></p:by><p:from><a:srgbClr val="112233"/></p:from><p:to><a:srgbClr val="AABBCC"/></p:to></p:animClr>',
        '<p:animEffect filter="fade" prLst="x=2">'+behavior(7)+'<p:progress><p:boolVal val="true"/></p:progress></p:animEffect>',
        '<p:animMotion origin="layout" path="M 0 0 L 1 1 E" pathEditMode="relative" rAng="2700000" ptsTypes="AA">'+behavior(8)+'<p:from x="10%" y="-20000"/><p:to x="100000" y="50%"/><p:rCtr x="50000" y="50000"/></p:animMotion>',
        '<p:animRot from="0" to="10800000" by="5400000">'+behavior(9)+'</p:animRot>',
        '<p:animScale zoomContents="1">'+behavior(10)+'<p:by x="200000" y="150000"/></p:animScale>',
        '<p:cmd type="call" cmd="play">'+behavior(11)+'</p:cmd>',
        '<p:set>'+behavior(12, '<p:spTgt spid="7"><p:txEl><p:pRg st="1" end="2"/></p:txEl></p:spTgt>')+'<p:to><p:intVal val="-3"/></p:to></p:set>',
        '<p:audio isNarration="true"><p:cMediaNode vol="75%" mute="1" numSld="3" showWhenStopped="false">'+ctime(13)+'<p:tgtEl><p:sndTgt r:embed="sound" name="Audio"/></p:tgtEl></p:cMediaNode></p:audio>',
        '<p:video fullScrn="true"><p:cMediaNode>'+ctime(14)+'<p:tgtEl><p:spTgt spid="7"/></p:tgtEl></p:cMediaNode></p:video>',
    ]
    graph = '<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" repeatCount="2500" repeatDur="5000" spd="200%" accel="25000" decel="10%" autoRev="true" fill="hold" nodeType="tmRoot"><p:stCondLst><p:cond evt="onClick" delay="250"><p:tgtEl><p:spTgt spid="7"/></p:tgtEl></p:cond></p:stCondLst><p:endSync evt="onEnd"><p:rtn val="all"/></p:endSync><p:iterate type="wd" backwards="true"><p:tmPct val="25%"/></p:iterate><p:childTnLst>'+''.join(operations)+'</p:childTnLst><p:subTnLst><p:par>'+ctime(15)+'</p:par></p:subTnLst></p:cTn></p:par></p:tnLst><p:bldLst><p:bldP spid="7" grpId="1"><p:tmplLst><p:tmpl><p:tnLst><p:par>'+ctime(16)+'</p:par></p:tnLst></p:tmpl></p:tmplLst></p:bldP><p:bldDgm spid="7" grpId="2"/><p:bldOleChart spid="7" grpId="3"/><p:bldGraphic spid="7" grpId="4"><p:bldSub><a:bldChart bld="series"/></p:bldSub></p:bldGraphic></p:bldLst><p:extLst><p:ext uri="retained"><u:unrecognized/></p:ext></p:extLst></p:timing>'
    transition = '<p:transition p14:dur="1250" advClick="false" advTm="4000"><p:fade/><p:sndAc><p:stSnd loop="true"><p:snd r:embed="sound" name="Audio"/></p:stSnd></p:sndAc></p:transition>'
    classic = ['blinds','checker','circle','dissolve','comb','cover','cut','diamond','fade','newsflash','plus','pull','push','random','randomBar','split','strips','wedge','wheel','wipe','zoom']
    contents = [transition+graph]+['<p:transition><p:'+n+'/></p:transition>' for n in classic]
    contents += ['<mc:AlternateContent><mc:Choice Requires="m"><p:transition><m:morph option="byWord"/></p:transition></mc:Choice><mc:Fallback><p:transition><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>', '<mc:AlternateContent><mc:Choice Requires="u"><p:transition><u:effect/></p:transition></mc:Choice><mc:Fallback><p:transition><p:wipe/></p:transition></mc:Fallback></mc:AlternateContent>', '<p:transition><p:sndAc><p:endSnd/></p:sndAc></p:transition>', '<u:timing><p:tnLst><p:par>'+ctime(90)+'</p:par></p:tnLst></u:timing>', '<mc:AlternateContent><mc:Choice Requires="p14"><p:timing><p:tnLst><p:par>'+ctime(91)+'</p:par></p:tnLst></p:timing></mc:Choice><mc:Fallback>'+graph+'</mc:Fallback></mc:AlternateContent>']
    contents += ['<p:timing><p:tnLst><p:animClr>'+behavior(92)+'<p:from><a:srgbClr val="112233"><a:alpha val="50%"/></a:srgbClr></p:from><p:to><a:scrgbClr r="100%" g="0%" b="50%"/></p:to><p:by><p:rgb r="25%" g="-50000" b="10000"/></p:by></p:animClr></p:tnLst></p:timing>', '<p:timing><p:tnLst><p:audio><p:cMediaNode>'+ctime(93)+'<p:tgtEl><p:sndTgt r:embed="isolated"/></p:tgtEl></p:cMediaNode></p:audio></p:tnLst></p:timing>']
    def rels(entries):
        return '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'+''.join(f'<Relationship Id="{i}" Type="{NS["r"]}/{kind}" Target="{target}"/>' for i,kind,target in entries)+'</Relationships>'
    namespaces=' '.join(f'xmlns:{k}="{v}"' for k,v in dict(NS, p14=P14,m=MORPH,mc=MC,u='urn:unsupported').items())
    parts = {'ppt/presentation.xml':'<p:presentation '+namespaces+'><p:sldIdLst>'+''.join(f'<p:sldId id="{256+i}" r:id="s{i}"/>' for i in range(len(contents)))+'</p:sldIdLst></p:presentation>', 'ppt/_rels/presentation.xml.rels':rels([(f's{i}','slide',f'slides/slide{i+1}.xml') for i in range(len(contents))]), 'ppt/media/audio.wav':b'RIFF\x00\x00\x00\x00WAVE'}
    for i,content in enumerate(contents):
        parts[f'ppt/slides/slide{i+1}.xml']='<p:sld '+namespaces+'><p:cSld><p:spTree/></p:cSld>'+content+'</p:sld>'
        parts[f'ppt/slides/_rels/slide{i+1}.xml.rels']=rels([('sound','audio','../media/audio.wav')])
    parts['ppt/slides/slide28.xml'] = parts['ppt/slides/slide28.xml'].replace(NS['p'], 'http://purl.oclc.org/ooxml/presentationml/main').replace(NS['a'], 'http://purl.oclc.org/ooxml/drawingml/main')
    parts['ppt/slides/_rels/slide29.xml.rels'] = rels([('isolated','audio','/payloads/audio.wav')])
    parts['payloads/audio.wav'] = b'RIFF\x00\x00\x00\x00WAVE'
    parts['ppt/slides/_rels/slide1.xml.rels'] = rels([('sound','audio','../media/audio.wav'),('layout','slideLayout','../slideLayouts/slideLayout1.xml')])
    parts['ppt/slideLayouts/slideLayout1.xml'] = '<p:sldLayout '+namespaces+'><p:cSld><p:spTree/></p:cSld>'+transition+graph+'</p:sldLayout>'
    parts['ppt/slideLayouts/_rels/slideLayout1.xml.rels'] = rels([('sound','audio','../media/audio.wav'),('master','slideMaster','../slideMasters/slideMaster1.xml')])
    parts['ppt/slideMasters/slideMaster1.xml'] = '<p:sldMaster '+namespaces+'><p:cSld><p:spTree/></p:cSld>'+transition+graph+'</p:sldMaster>'
    parts['ppt/slideMasters/_rels/slideMaster1.xml.rels'] = rels([('sound','audio','../media/audio.wav')])
    path=OUT/'timing.pptx' 
    with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as z:
        for name,value in parts.items():z.writestr(name,value)
    result=subprocess.run([str(binary),'duke-slides',str(path),str(OUT/'timing.json.gz'),'0','full'],capture_output=True,text=True,check=True)
    record=json.loads(result.stdout.splitlines()[-1]);assert record['status']=='ok',record
    model=json.load(gzip.open(OUT/'timing.json.gz','rt'));slides=model['slides'];t=slides[0]['timing'];root=t['nodes'][0]['time'];nodes={n['kind']:n for n in root['children']};p=lambda n:nodes[n]['parameters']
    checks={
      'classic_transition_types':[s['transition']['effect']['kind'] for s in slides[1:22]]==classic,
      'transition_defaults':slides[19]['transition']['effect']['spokes']==4 and slides[20]['transition']['effect']['direction']=='l' and slides[16]['transition']['effect']['orientation']=='horz',
      'transition_duration_advancement':slides[0]['transition']['durationMs']==1250 and slides[0]['transition']['advanceAfterMs']==4000 and not slides[0]['transition']['advanceOnClick'],
      'transition_sound':slides[0]['transition']['sound']['source']['assetId']=='ppt/media/audio.wav' and slides[0]['transition']['sound']['loopSound'],
      'morph_namespace_choice':slides[22]['transition']['effect']['kind']=='morph' and slides[22]['transition']['effect']['parameters']['option']=='byWord',
      'unsupported_choice_fallback':slides[23]['transition']['effect']['kind']=='wipe',
      'stop_sound':slides[24]['transition']['sound']['action']=='stop',
      'namespace_collision_rejected':'timing' not in slides[25],
      'timing_choice_single_branch':slides[26]['timing']['nodes'][0]['time']['id']==91,
      'all_time_node_kinds':len(nodes)==13,
      'common_time_units':root['durationMs']=='indefinite' and root['repeatCount']==2.5 and root['speed']==2 and root['acceleration']==.25 and root['deceleration']==.1,
      'conditions_and_subnodes':root['startConditions'][0]['target']['shapeId']=='7' and root['endSync']['runtimeNode']=='all' and root['subNodes'][0]['time']['id']==15,
      'iteration':root['iterate']['intervalFraction']==.25 and root['iterate']['backwards'],
      'sequence_links':nodes['seq']['sequence']['nextConditions'][0]['timeNodeId']==2,
      'keyframe_values':p('anim')['keyframes'][0]['time']==.25 and p('anim')['keyframes'][1]['time']==1 and p('anim')['keyframes'][1]['value']['value']=='A & B',
      'color_and_delta':p('animClr')['from']['value']['value']=='#112233' and p('animClr')['by']['value']==[90,.2,-.1],
      'effect_default':p('animEffect')['transition']=='in' and p('animEffect')['progress']['value'],
      'motion_units':p('animMotion')['from']['value']==dict(x=.1,y=-.2) and p('animMotion')['rotationDegrees']==45,
      'rotation_and_scale':p('animRot')['to']['value']==180 and p('animScale')['by']['value']==dict(x=2,y=1.5),
      'command_and_set':p('cmd')['command']=='play' and p('set')['to']['value']==-3,
      'text_target':nodes['set']['behavior']['target']['text']==dict(kind='paragraph',start=1,end=2),
      'media_defaults_and_sound':nodes['audio']['media']['target']['sound']['assetId']=='ppt/media/audio.wav' and nodes['audio']['media']['volume']==.75 and nodes['video']['media']['volume']==.5 and nodes['video']['media']['fullScreen'],
      'builds_and_templates':len(t['builds'])==4 and t['builds'][0]['build']=='whole' and t['builds'][0]['templates'][0]['nodes'][0]['time']['id']==16 and t['builds'][3]['graphicBuild']['build']=='series',
      'strict_color_percentages':slides[27]['timing']['nodes'][0]['parameters']['from']['value']['alpha']==.5 and slides[27]['timing']['nodes'][0]['parameters']['to']['value']['value']=='#FF0080',
      'rgb_delta_percentages':slides[27]['timing']['nodes'][0]['parameters']['by']['value']==[.25,-.5,.1],
      'nonstandard_audio_payload':slides[28]['timing']['nodes'][0]['media']['target']['sound']['assetId']=='payloads/audio.wav' and bytes(model['assets']['payloads/audio.wav']['data'])==parts['payloads/audio.wav'],
      'layout_timing_and_sound':model['layouts'][0]['timing']['nodes'][0]['time']['id']==1 and model['layouts'][0]['transition']['sound']['source']['assetId']=='ppt/media/audio.wav',
      'master_timing_and_sound':model['masters'][0]['timing']['nodes'][0]['time']['id']==1 and model['masters'][0]['transition']['sound']['source']['assetId']=='ppt/media/audio.wav',
      'unsupported_retained':bool(t['unsupported']) and 'unrecognized' in t['sourceXml'],
    }
    for s in slides:
        for key in ['transition','timing']:
            if key in s:ET.fromstring(s[key]['sourceXml'])
    summary=dict(checks=checks,passed=sum(checks.values()),total=len(checks),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    (OUT/'results.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2));assert all(checks.values())

if __name__=='__main__':main()
