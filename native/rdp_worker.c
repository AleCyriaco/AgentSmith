#include <freerdp/freerdp.h>
#include <freerdp/gdi/gdi.h>
#include <freerdp/input.h>
#include <freerdp/settings.h>
#include <winpr/synch.h>
#include <winpr/wlog.h>
#include <winpr/sysinfo.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/select.h>
#include <signal.h>
#include <ctype.h>

static char expected_fp[512];
static void status_msg(const char* s) { uint32_t n=(uint32_t)strlen(s); fputc('S',stdout); fwrite(&n,4,1,stdout); fwrite(s,1,n,stdout); fflush(stdout); }
static void norm(const char* in,char* out,size_t cap) { size_t n=0; for(;*in&&n+1<cap;in++) if(isxdigit((unsigned char)*in)) out[n++]=(char)tolower(*in);out[n]=0; }
static DWORD certificate(freerdp* f,const char* host,UINT16 port,const char* cn,const char* subject,const char* issuer,const char* fp,DWORD flags) {
 (void)f;(void)host;(void)port;(void)cn;(void)subject;(void)issuer;
 if(flags & VERIFY_CERT_FLAG_FP_IS_PEM) {status_msg("Certificado recebido em formato incompatível. Configure confiança no certificado do servidor.");return 0;}
 char a[512],b[512];norm(expected_fp,a,sizeof(a));norm(fp,b,sizeof(b));
 if(strlen(a)>=40 && strcmp(a,b)==0) return 2;
 char msg[1024];snprintf(msg,sizeof(msg),"Certificado não confirmado. Confira a impressão digital com o administrador e salve no cadastro: %s",fp);status_msg(msg);return 0;
}
static DWORD changed_certificate(freerdp* f,const char* host,UINT16 port,const char* cn,const char* subject,const char* issuer,const char* fp,const char* oldsubject,const char* oldissuer,const char* oldfp,DWORD flags) {
 (void)oldsubject;(void)oldissuer;(void)oldfp;return certificate(f,host,port,cn,subject,issuer,fp,flags);
}
static BOOL resize(rdpContext* c) {return gdi_resize(c->gdi,freerdp_settings_get_uint32(c->settings,FreeRDP_DesktopWidth),freerdp_settings_get_uint32(c->settings,FreeRDP_DesktopHeight));}
static BOOL post_connect(freerdp* f) {if(!gdi_init(f,PIXEL_FORMAT_BGRA32))return FALSE;f->context->update->DesktopResize=resize;return TRUE;}
static void frame(freerdp* f) {rdpGdi* g=f->context->gdi;if(!g||!g->primary_buffer||g->width<1||g->height<1||g->width>4096||g->height>2160)return;uint32_t w=g->width,h=g->height,n=w*h*4;fputc('F',stdout);fwrite(&w,4,1,stdout);fwrite(&h,4,1,stdout);fwrite(&n,4,1,stdout);for(uint32_t y=0;y<h;y++)fwrite(g->primary_buffer+y*g->stride,1,w*4,stdout);fflush(stdout);}
static int line(char* b,size_t n) {if(!fgets(b,(int)n,stdin))return 0;size_t k=strlen(b);if(k&&b[k-1]=='\n')b[--k]=0;else return 0;if(k&&b[k-1]=='\r')b[--k]=0;return 1;}
int main(void) {
 signal(SIGPIPE,SIG_DFL);setvbuf(stdin,NULL,_IONBF,0);setvbuf(stdout,NULL,_IONBF,0);WLog_SetLogLevel(WLog_GetRoot(),WLOG_OFF);
 char host[1024],user[1024],domain[1024],pass[4096],portbuf[32],displaybuf[64];
 if(!line(host,sizeof(host))||!line(portbuf,sizeof(portbuf))||!line(user,sizeof(user))||!line(domain,sizeof(domain))||!line(pass,sizeof(pass))||!line(expected_fp,sizeof(expected_fp))||!line(displaybuf,sizeof(displaybuf)))return 2;
 unsigned width=0,height=0,scale=0,interval=300;char extra;
 if(sscanf(displaybuf,"%u %u %u %u %c",&width,&height,&scale,&interval,&extra)!=4 || interval<100 || interval>2000 || width<800 || width>2560 || height<600 || height>1440 || !(scale==100||scale==125||scale==150||scale==200)){status_msg("Resolução ou escala inválida.");return 2;}
 freerdp* f=freerdp_new();if(!f)return 2;if(!freerdp_context_new(f)){freerdp_free(f);return 2;}
 f->PostConnect=post_connect;f->VerifyCertificateEx=certificate;f->VerifyChangedCertificateEx=changed_certificate;
 rdpSettings* s=f->context->settings;
 freerdp_settings_set_string(s,FreeRDP_ServerHostname,host);freerdp_settings_set_uint32(s,FreeRDP_ServerPort,(UINT32)atoi(portbuf));
 freerdp_settings_set_string(s,FreeRDP_Username,user);freerdp_settings_set_string(s,FreeRDP_Domain,domain);freerdp_settings_set_string(s,FreeRDP_Password,pass);memset(pass,0,sizeof(pass));
 freerdp_settings_set_uint32(s,FreeRDP_DesktopWidth,width);freerdp_settings_set_uint32(s,FreeRDP_DesktopHeight,height);freerdp_settings_set_uint32(s,FreeRDP_ColorDepth,32);
 freerdp_settings_set_uint32(s,FreeRDP_DesktopScaleFactor,scale);freerdp_settings_set_uint32(s,FreeRDP_DeviceScaleFactor,100);
 freerdp_settings_set_bool(s,FreeRDP_SoftwareGdi,TRUE);freerdp_settings_set_bool(s,FreeRDP_AsyncUpdate,FALSE);freerdp_settings_set_bool(s,FreeRDP_AsyncChannels,FALSE);
 freerdp_settings_set_bool(s,FreeRDP_CertificateCallbackPreferPEM,FALSE);
 freerdp_settings_set_bool(s,FreeRDP_NlaSecurity,TRUE);freerdp_settings_set_bool(s,FreeRDP_TlsSecurity,TRUE);
 if(!freerdp_connect(f)){char msg[256];snprintf(msg,sizeof(msg),"Falha RDP: %s",freerdp_get_last_error_name(freerdp_get_last_error(f->context)));status_msg(msg);freerdp_context_free(f);freerdp_free(f);return 3;}
 status_msg("connected");uint64_t last=0;char cmd[256];BOOL running=TRUE;
 while(running&&!freerdp_shall_disconnect_context(f->context)) {
  HANDLE handles[64];DWORD count=freerdp_get_event_handles(f->context,handles,64);if(!count)break;WaitForMultipleObjects(count,handles,FALSE,10);if(!freerdp_check_event_handles(f->context))break;
  fd_set fd;FD_ZERO(&fd);FD_SET(0,&fd);struct timeval tv={0,0};
  for(int drained=0;drained<512;drained++){FD_ZERO(&fd);FD_SET(0,&fd);tv.tv_sec=0;tv.tv_usec=0;if(select(1,&fd,NULL,NULL,&tv)<=0)break;if(!line(cmd,sizeof(cmd))){running=FALSE;break;}unsigned a=0,b=0,c=0;
   if(strcmp(cmd,"quit")==0)running=FALSE;
   else if(sscanf(cmd,"interval %u",&a)==1 && a>=100 && a<=2000)interval=a;
   else if(sscanf(cmd,"mouse %u %u %u",&a,&b,&c)==3 && a<=65535 && b<(unsigned)f->context->gdi->width && c<(unsigned)f->context->gdi->height)freerdp_input_send_mouse_event(f->context->input,(UINT16)a,(UINT16)b,(UINT16)c);
   else if(sscanf(cmd,"key %u %u",&a,&b)==2 && a<=511)freerdp_input_send_keyboard_event_ex(f->context->input,b!=0,FALSE,a);
   else if(sscanf(cmd,"unicode %u %u",&a,&b)==2 && a<=65535)freerdp_input_send_unicode_keyboard_event(f->context->input,b?0:KBD_FLAGS_RELEASE,(UINT16)a);
  }
  uint64_t tick=GetTickCount64();if(tick-last>=interval){frame(f);last=tick;}
 }
 status_msg("disconnected");freerdp_disconnect(f);gdi_free(f);freerdp_context_free(f);freerdp_free(f);return 0;
}
